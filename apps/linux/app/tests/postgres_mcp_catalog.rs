#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, Environment, TlsConfig};
use bookie_mcp::{ConnectionProvider, McpBridge, TokenPermissions, TokenStore, dispatch};
use bookie_policy::Principal;
use bookie_storage::SavedConnection;
use drivers_postgres::PgDriver;
use serde_json::json;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

struct RestrictedPostgresProvider {
    connection_id: Uuid,
    saved: SavedConnection,
    connection: Arc<dyn Connection>,
}

#[async_trait]
impl ConnectionProvider for RestrictedPostgresProvider {
    async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
        Ok(vec![self.saved.clone()])
    }

    async fn connection(&self, connection_id: Uuid, _principal: Principal) -> Result<Arc<dyn Connection>, String> {
        (connection_id == self.connection_id)
            .then(|| self.connection.clone())
            .ok_or_else(|| "unexpected PostgreSQL connection id".into())
    }
}

fn saved_connection(connection_id: Uuid, host: String, port: u16) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id: connection_id,
        name: "Restricted PostgreSQL fixture".into(),
        driver_id: "postgres".into(),
        host,
        port,
        socket_dir: None,
        socket_path: None,
        database: "postgres".into(),
        username: "bookie_restricted".into(),
        use_tls: false,
        tls_mode: None,
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: true,
        auth_mode: Default::default(),
        environment: Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        attached_databases: Vec::new(),
        tls_strict_encryption: false,
    }
}

async fn restricted_context(host: String, port: u16) -> (Uuid, McpBridge, bookie_mcp::McpToken, tempfile::TempDir) {
    let connection = PgDriver
        .connect(ConnectOptions {
            host: host.clone(),
            port,
            database: "postgres".into(),
            username: "bookie_restricted".into(),
            password: secrecy::SecretString::new("restricted".to_string().into()),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .expect("restricted PostgreSQL connection");
    let connection_id = Uuid::new_v4();
    let provider = Arc::new(RestrictedPostgresProvider {
        connection_id,
        saved: saved_connection(connection_id, host, port),
        connection: Arc::from(connection),
    });
    let directory = tempfile::tempdir().expect("token directory");
    let tokens = Arc::new(TokenStore::open(directory.path().join("tokens.json")).expect("token store"));
    let (_, plaintext) = tokens
        .issue(
            "Restricted PostgreSQL catalog test".into(),
            TokenPermissions::ReadOnly,
            vec![connection_id],
            None,
        )
        .expect("issue read-only token");
    let bridge = McpBridge::new(provider, tokens);
    let token = bridge.authenticate(&plaintext).expect("authenticate token");
    (connection_id, bridge, token, directory)
}

async fn postgres_fixture() -> (
    testcontainers::ContainerAsync<Postgres>,
    String,
    u16,
    Arc<dyn Connection>,
) {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("PostgreSQL fixture");
    let host = container.get_host().await.expect("PostgreSQL host").to_string();
    let port = container.get_host_port_ipv4(5432).await.expect("PostgreSQL port");
    let connection = PgDriver
        .connect(ConnectOptions {
            host: host.clone(),
            port,
            database: "postgres".into(),
            username: "postgres".into(),
            password: secrecy::SecretString::new("postgres".to_string().into()),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .expect("admin PostgreSQL connection");
    (container, host, port, Arc::from(connection))
}

async fn seed_catalog(admin: &dyn Connection) {
    for sql in [
        "CREATE ROLE bookie_restricted LOGIN PASSWORD 'restricted'",
        "CREATE SCHEMA bookie_visible",
        "CREATE SCHEMA bookie_hidden",
        "CREATE TABLE bookie_visible.allowed (id integer PRIMARY KEY)",
        "CREATE TABLE bookie_visible.denied (id integer)",
        "CREATE TABLE bookie_visible.partial (id integer, private_value text)",
        "CREATE TABLE bookie_hidden.secret (id integer PRIMARY KEY)",
        "CREATE FUNCTION bookie_visible.open_fn() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        "CREATE FUNCTION bookie_hidden.secret_fn() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        "GRANT USAGE ON SCHEMA bookie_visible TO bookie_restricted",
        "GRANT SELECT ON bookie_visible.allowed TO bookie_restricted",
        "GRANT SELECT (id) ON bookie_visible.partial TO bookie_restricted",
        "GRANT EXECUTE ON FUNCTION bookie_visible.open_fn() TO bookie_restricted",
    ] {
        admin.execute(sql).await.expect("seed role-scoped catalog");
    }
}

async fn assert_restricted_catalog(bridge: &McpBridge, token: &bookie_mcp::McpToken, connection_id: Uuid) {
    let tables = dispatch(
        bridge,
        token,
        "list_tables",
        json!({"connection_id": connection_id.to_string()}),
    )
    .await
    .expect("list restricted tables");
    let visible_tables: Vec<_> = tables
        .as_array()
        .expect("table list")
        .iter()
        .filter(|table| {
            table["schema"]
                .as_str()
                .is_some_and(|schema| schema.starts_with("bookie_"))
        })
        .cloned()
        .collect();
    assert_eq!(
        visible_tables,
        vec![
            json!({"schema":"bookie_visible","name":"allowed"}),
            json!({"schema":"bookie_visible","name":"partial"}),
        ]
    );

    let partial_columns = dispatch(
        bridge,
        token,
        "describe_table",
        json!({"connection_id": connection_id.to_string(), "schema": "bookie_visible", "table": "partial"}),
    )
    .await
    .expect("describe table with partial column access");
    assert_eq!(partial_columns.as_array().map(Vec::len), Some(1));
    assert_eq!(partial_columns[0]["name"], "id");

    let routines = dispatch(
        bridge,
        token,
        "list_objects",
        json!({"connection_id": connection_id.to_string(), "kind": "routine"}),
    )
    .await
    .expect("list restricted routines");
    let visible_routines: Vec<_> = routines
        .as_array()
        .expect("routine list")
        .iter()
        .filter(|routine| {
            routine["schema"]
                .as_str()
                .is_some_and(|schema| schema.starts_with("bookie_"))
        })
        .cloned()
        .collect();
    assert_eq!(
        visible_routines,
        vec![json!({"schema":"bookie_visible","name":"open_fn"})]
    );

    let hidden_columns = dispatch(
        bridge,
        token,
        "describe_table",
        json!({"connection_id": connection_id.to_string(), "schema": "bookie_hidden", "table": "secret"}),
    )
    .await
    .expect("describe hidden table without exposing metadata");
    assert_eq!(hidden_columns, json!([]));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mcp_catalog_tools_follow_restricted_postgres_role_visibility() {
    let (_container, host, port, admin) = postgres_fixture().await;
    seed_catalog(admin.as_ref()).await;
    let (connection_id, bridge, token, _directory) = restricted_context(host, port).await;
    assert_restricted_catalog(&bridge, &token, connection_id).await;
}
