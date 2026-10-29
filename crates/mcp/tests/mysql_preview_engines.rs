#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use drivers_mysql::MysqlDriver;
use tablepro_core::{ConnectOptions, Connection, DatabaseDriver, Environment, TlsConfig, Value};
use tablepro_mcp::{ConnectionProvider, McpBridge, TokenPermissions, TokenStore};
use tablepro_policy::Principal;
use tablepro_storage::SavedConnection;
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

#[tokio::test]
#[ignore = "requires docker"]
async fn a_preview_is_refused_for_a_myisam_table_and_still_runs_for_innodb() {
    let container = Mysql::default()
        .with_tag("8.4")
        .with_env_var("MYSQL_ROOT_PASSWORD", "tablepro_test")
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
            password: secrecy::SecretString::new("tablepro_test".to_string().into()),
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

    let refused = bridge
        .execute_write(&token, connection_id, "UPDATE legacy SET v = 2 WHERE id = 1", true)
        .await;
    let allowed = bridge
        .execute_write(&token, connection_id, "UPDATE modern SET v = 2 WHERE id = 1", true)
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
