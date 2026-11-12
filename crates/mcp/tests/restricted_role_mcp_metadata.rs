#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, Environment, TlsConfig};
use bookie_mcp::{ConnectionProvider, McpBridge, McpToken, TokenPermissions, TokenStore};
use bookie_policy::{
    AuditError, AuditEvent, AuditSink, AuditState, AutoApproveSink, GuardContext, PolicyConfig, PolicyGuard, Principal,
};
use bookie_storage::SavedConnection;
use drivers_postgres::PgDriver;
use serde_json::{Value as JsonValue, json};
use testcontainers::ImageExt;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use uuid::Uuid;

const SEED: &[&str] = &[
    "CREATE ROLE mcp_restricted LOGIN PASSWORD 'restricted'",
    "CREATE SCHEMA bookie_visible",
    "CREATE SCHEMA bookie_hidden",
    "CREATE TABLE bookie_visible.shown (id int PRIMARY KEY, note text)",
    "CREATE TABLE bookie_visible.denied (id int PRIMARY KEY, secret text)",
    "CREATE MATERIALIZED VIEW bookie_visible.shown_mat AS SELECT id FROM bookie_visible.shown",
    "CREATE MATERIALIZED VIEW bookie_visible.denied_mat AS SELECT id FROM bookie_visible.denied",
    "CREATE TABLE bookie_hidden.secret (id int PRIMARY KEY, hidden_column text)",
    "INSERT INTO bookie_visible.shown VALUES (1, 'a')",
    "INSERT INTO bookie_hidden.secret VALUES (1, 'x')",
    "CREATE FUNCTION bookie_visible.open_fn() RETURNS int LANGUAGE sql AS 'SELECT 1'",
    "CREATE FUNCTION bookie_hidden.secret_fn() RETURNS int LANGUAGE sql AS 'SELECT 42'",
    "GRANT USAGE ON SCHEMA bookie_visible TO mcp_restricted",
    "GRANT SELECT ON bookie_visible.shown, bookie_visible.shown_mat TO mcp_restricted",
];

#[derive(Default)]
struct NullAudit;

#[async_trait]
impl AuditSink for NullAudit {
    async fn record(&self, _event: AuditEvent) -> Result<(), AuditError> {
        Ok(())
    }
}

struct Provider {
    restricted_id: Uuid,
    admin_id: Uuid,
    restricted: Arc<dyn Connection>,
    admin: Arc<dyn Connection>,
}

fn saved(id: Uuid, user: &str, port: u16) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id,
        name: user.into(),
        driver_id: "postgres".into(),
        host: "127.0.0.1".into(),
        port,
        socket_dir: None,
        socket_path: None,
        database: "postgres".into(),
        username: user.into(),
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

#[async_trait]
impl ConnectionProvider for Provider {
    async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
        Ok(vec![
            saved(self.restricted_id, "mcp_restricted", 0),
            saved(self.admin_id, "postgres", 0),
        ])
    }

    async fn connection(&self, connection_id: Uuid, principal: Principal) -> Result<Arc<dyn Connection>, String> {
        let inner = if connection_id == self.restricted_id {
            self.restricted.clone()
        } else if connection_id == self.admin_id {
            self.admin.clone()
        } else {
            return Err("unknown connection".into());
        };
        Ok(Arc::new(PolicyGuard::new(
            inner,
            GuardContext {
                connection_id,
                connection_name: "fixture".into(),
                driver_id: "postgres".into(),
                environment: Environment::Local,
                read_only: true,
                principal,
                policy: Arc::new(PolicyConfig::default()),
                approval: Arc::new(AutoApproveSink),
                audit: Arc::new(NullAudit),
                audit_state: Arc::new(AuditState::new()),
            },
        )))
    }
}

fn options(host: &str, port: u16, user: &str, password: &str) -> ConnectOptions {
    ConnectOptions {
        host: host.into(),
        port,
        database: "postgres".into(),
        username: user.into(),
        password: secrecy::SecretString::new(password.to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    }
}

struct Session {
    bridge: McpBridge,
    token: McpToken,
    restricted: String,
    admin: String,
    _dir: tempfile::TempDir,
}

impl Session {
    async fn call(&self, tool: &str, connection: &str, mut args: JsonValue) -> Result<JsonValue, String> {
        args["connection_id"] = json!(connection);
        bookie_mcp::dispatch(&self.bridge, &self.token, tool, args).await
    }
}

fn names(value: &JsonValue, field: &str) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item[field].as_str().map(str::to_string))
        .collect()
}

fn session(restricted: Arc<dyn Connection>, admin: Arc<dyn Connection>) -> Session {
    let restricted_id = Uuid::new_v4();
    let admin_id = Uuid::new_v4();
    let provider = Arc::new(Provider {
        restricted_id,
        admin_id,
        restricted,
        admin,
    });
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(TokenStore::open(dir.path().join("tokens.json")).unwrap());
    let (_meta, plaintext) = store
        .issue(
            "restricted-role".into(),
            TokenPermissions::ReadOnly,
            vec![restricted_id, admin_id],
            None,
        )
        .unwrap();
    let bridge = McpBridge::new(provider, store);
    let token = bridge.authenticate(&plaintext).unwrap();
    Session {
        bridge,
        token,
        restricted: restricted_id.to_string(),
        admin: admin_id.to_string(),
        _dir: dir,
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_restricted_role_sees_only_its_objects_through_every_mcp_metadata_tool() {
    let container = Postgres::default().with_tag("16-alpine").start().await.unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin = PgDriver
        .connect(options(&host, port, "postgres", "postgres"))
        .await
        .unwrap();
    for statement in SEED {
        admin.execute(statement).await.expect(statement);
    }
    let restricted = PgDriver
        .connect(options(&host, port, "mcp_restricted", "restricted"))
        .await
        .unwrap();
    let s = session(Arc::from(restricted), Arc::from(admin));
    let (r, a) = (s.restricted.as_str(), s.admin.as_str());

    let tables = names(&s.call("list_tables", r, json!({})).await.unwrap(), "name");
    assert_eq!(tables, ["shown"], "{tables:?}");
    let admin_tables = names(&s.call("list_tables", a, json!({})).await.unwrap(), "name");
    assert!(admin_tables.contains(&"secret".to_string()) && admin_tables.contains(&"denied".to_string()));

    let routines = s
        .call("list_objects", r, json!({"kind": "routine", "schema": "bookie_hidden"}))
        .await
        .unwrap();
    assert_eq!(routines, json!([]), "a schema without USAGE lists no routine");
    let visible = names(
        &s.call(
            "list_objects",
            r,
            json!({"kind": "routine", "schema": "bookie_visible"}),
        )
        .await
        .unwrap(),
        "name",
    );
    assert_eq!(visible, ["open_fn"]);

    for (schema, table) in [
        ("bookie_hidden", "secret"),
        ("bookie_visible", "denied"),
        ("bookie_visible", "denied_mat"),
    ] {
        let args = json!({"schema": schema, "table": table});
        assert_eq!(
            s.call("describe_table", r, args.clone()).await.unwrap(),
            json!([]),
            "{table}"
        );
        let described = s.call("table_schema", r, args.clone()).await.unwrap();
        assert_eq!(described["columns"], json!([]), "{table}");
        assert_eq!(described["primary_key"], json!([]), "{table}: no invented key");
        assert_eq!(described["indexes"], json!([]), "{table}");
        assert_eq!(described["foreign_keys"], json!([]), "{table}");
        for tool in ["count_rows", "browse_table"] {
            let error = s.call(tool, r, args.clone()).await.unwrap_err();
            assert!(!error.contains("row_count"), "{tool} {table}: {error}");
        }
        let admin_view = s.call("table_schema", a, args).await.unwrap();
        assert!(
            !admin_view["columns"].as_array().unwrap().is_empty(),
            "{table} is real for the owner"
        );
    }

    let shown = s
        .call("table_schema", r, json!({"schema": "bookie_visible", "table": "shown"}))
        .await
        .unwrap();
    assert_eq!(shown["primary_key"], json!(["id"]));
    let count = s
        .call("count_rows", r, json!({"schema": "bookie_visible", "table": "shown"}))
        .await
        .unwrap();
    assert_eq!(count["row_count"]["value"], "1");

    let open = s
        .call(
            "routine_source",
            r,
            json!({"schema": "bookie_visible", "name": "open_fn"}),
        )
        .await
        .unwrap();
    assert_eq!(open["found"], true);
    let secret = s
        .call(
            "routine_source",
            r,
            json!({"schema": "bookie_hidden", "name": "secret_fn"}),
        )
        .await
        .unwrap();
    assert_eq!(secret, json!({"found": false}));
    let admin_secret = s
        .call(
            "routine_source",
            a,
            json!({"schema": "bookie_hidden", "name": "secret_fn"}),
        )
        .await
        .unwrap();
    assert_eq!(admin_secret["found"], true);

    let context = s.call("schema_context", r, json!({})).await.unwrap();
    let included: Vec<String> = context["tables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|table| {
            format!(
                "{}.{}",
                table["schema"].as_str().unwrap(),
                table["name"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(included, ["bookie_visible.shown"], "{context}");
    assert_eq!(context["tables"][0]["primary_key"], json!(["id"]));
    assert_eq!(context["skipped_tables"], 0);
    let admin_context = s.call("schema_context", a, json!({})).await.unwrap();
    assert_eq!(admin_context["included_tables"], 3, "{admin_context}");
}
