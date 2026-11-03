#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, Environment, TlsConfig, Value};
use bookie_mcp::{ConnectionProvider, McpBridge, TokenPermissions, TokenStore};
use bookie_policy::Principal;
use bookie_storage::SavedConnection;
use drivers_mysql::MysqlDriver;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::mysql::Mysql;
use uuid::Uuid;

struct Provider {
    saved: SavedConnection,
    connection: Arc<dyn Connection>,
}

#[async_trait]
impl ConnectionProvider for Provider {
    async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
        Ok(vec![self.saved.clone()])
    }

    async fn connection(&self, _: Uuid, _: Principal) -> Result<Arc<dyn Connection>, String> {
        Ok(self.connection.clone())
    }
}

struct Fixture {
    _container: testcontainers::ContainerAsync<Mysql>,
    bridge: McpBridge,
    token: bookie_mcp::McpToken,
    connection_id: Uuid,
    connection: Arc<dyn Connection>,
    _directory: tempfile::TempDir,
}

async fn fixture() -> Fixture {
    let container = Mysql::default()
        .with_tag("8.4")
        .with_env_var("MYSQL_ROOT_PASSWORD", "bookie_test")
        .start()
        .await
        .unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(3306).await.unwrap();
    let connection = MysqlDriver
        .connect(ConnectOptions {
            host: host.clone(),
            port,
            database: "test".into(),
            username: "root".into(),
            password: secrecy::SecretString::new("bookie_test".to_string().into()),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .unwrap();
    for sql in [
        "CREATE TABLE legacy (id INT PRIMARY KEY, v INT) ENGINE=MyISAM",
        "CREATE TABLE modern (id INT PRIMARY KEY, v INT) ENGINE=InnoDB",
        "INSERT INTO legacy VALUES (1, 1)",
        "INSERT INTO modern VALUES (1, 1)",
    ] {
        connection.execute(sql).await.unwrap();
    }
    let connection: Arc<dyn Connection> = Arc::from(connection);
    let connection_id = Uuid::new_v4();
    let saved = SavedConnection {
        auth_source: None,
        id: connection_id,
        name: "MySQL preview fixture".into(),
        driver_id: "mysql".into(),
        host,
        port,
        socket_dir: None,
        socket_path: None,
        database: "test".into(),
        username: "root".into(),
        use_tls: false,
        tls_mode: None,
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: false,
        auth_mode: Default::default(),
        environment: Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        attached_databases: Vec::new(),
    };
    let directory = tempfile::tempdir().unwrap();
    let tokens = Arc::new(TokenStore::open(directory.path().join("tokens.json")).unwrap());
    let (_metadata, plaintext) = tokens
        .issue("preview".into(), TokenPermissions::ReadWrite, vec![connection_id], None)
        .unwrap();
    let bridge = McpBridge::new(
        Arc::new(Provider {
            saved,
            connection: connection.clone(),
        }),
        tokens,
    );
    let token = bridge.authenticate(&plaintext).unwrap();
    Fixture {
        _container: container,
        bridge,
        token,
        connection_id,
        connection,
        _directory: directory,
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_preview_is_refused_for_a_myisam_table_and_still_runs_for_innodb() {
    let fixture = fixture().await;
    let (bridge, token, connection_id, connection) = (
        &fixture.bridge,
        &fixture.token,
        fixture.connection_id,
        &fixture.connection,
    );

    let refused = bridge
        .execute_write(token, connection_id, "UPDATE legacy SET v = 2 WHERE id = 1", true)
        .await;
    let allowed = bridge
        .execute_write(token, connection_id, "UPDATE modern SET v = 2 WHERE id = 1", true)
        .await;

    assert!(refused.is_err(), "a MyISAM preview cannot be rolled back: {refused:?}");
    assert!(format!("{allowed:?}").contains("rolled_back: true"), "{allowed:?}");
    let rows = connection
        .query("SELECT (SELECT v FROM legacy), (SELECT v FROM modern)")
        .await
        .unwrap()
        .rows;
    assert_eq!(rows, vec![vec![Value::Int(1), Value::Int(1)]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_preview_is_refused_for_statements_that_commit_on_their_own_and_nothing_runs() {
    let fixture = fixture().await;
    let (bridge, token, connection_id, connection) = (
        &fixture.bridge,
        &fixture.token,
        fixture.connection_id,
        &fixture.connection,
    );
    for sql in [
        "TRUNCATE TABLE modern",
        "ALTER TABLE modern ADD COLUMN note INT",
        "CREATE TABLE created_by_preview (id INT)",
        "DROP TABLE modern",
        "UPDATE modern SET v = 2 WHERE id = 1; DROP TABLE modern",
        "DELETE FROM modern WHERE id = 1; COMMIT",
    ] {
        let refused = bridge.execute_write_structured(token, connection_id, sql, true).await;
        let Err(error) = refused else {
            panic!("{sql} must not be previewed: {refused:?}");
        };
        assert_eq!(error.code, "preview_not_transactional", "{sql}: {error:?}");
    }
    let rows = connection
        .query("SELECT (SELECT COUNT(*) FROM modern), (SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = 'created_by_preview'), (SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = 'modern' AND column_name = 'note')")
        .await
        .unwrap()
        .rows;
    assert_eq!(rows, vec![vec![Value::Int(1), Value::Int(0), Value::Int(0)]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_preview_is_refused_for_a_myisam_table_in_another_database() {
    let fixture = fixture().await;
    let (bridge, token, connection_id, connection) = (
        &fixture.bridge,
        &fixture.token,
        fixture.connection_id,
        &fixture.connection,
    );
    for sql in [
        "CREATE DATABASE other_db",
        "CREATE TABLE other_db.legacy_elsewhere (id INT PRIMARY KEY, v INT) ENGINE=MyISAM",
        "INSERT INTO other_db.legacy_elsewhere VALUES (1, 1)",
    ] {
        connection.execute(sql).await.unwrap();
    }
    let refused = bridge
        .execute_write_structured(
            token,
            connection_id,
            "UPDATE other_db.legacy_elsewhere SET v = 2 WHERE id = 1",
            true,
        )
        .await;
    let Err(error) = refused else {
        panic!("a MyISAM preview in another database cannot be rolled back: {refused:?}");
    };
    assert_eq!(error.code, "preview_not_transactional", "{error:?}");
    let rows = connection
        .query("SELECT v FROM other_db.legacy_elsewhere WHERE id = 1")
        .await
        .unwrap()
        .rows;
    assert_eq!(rows, vec![vec![Value::Int(1)]]);
    connection
        .execute("CREATE TABLE other_db.modern_elsewhere (id INT PRIMARY KEY, v INT) ENGINE=InnoDB")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO other_db.modern_elsewhere VALUES (1, 1)")
        .await
        .unwrap();
    for sql in [
        "UPDATE other_db.modern_elsewhere SET v = 2 WHERE id = 1",
        "UPDATE modern SET v = (SELECT COUNT(*) FROM information_schema.tables) WHERE id = 1",
    ] {
        let allowed = bridge.execute_write_structured(token, connection_id, sql, true).await;
        assert!(
            format!("{allowed:?}").contains("rolled_back: true"),
            "{sql}: {allowed:?}"
        );
    }
}
