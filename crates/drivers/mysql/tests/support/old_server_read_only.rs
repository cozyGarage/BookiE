use bookie_core::{ConnectOptions, DatabaseDriver, TlsConfig, Value};
use drivers_mysql::MysqlDriver;
use secrecy::ExposeSecret;
use sqlx::Connection;
use sqlx::mysql::{MySqlConnectOptions, MySqlSslMode};
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::{GenericImage, ImageExt};
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
#[ignore = "requires docker and downloads mysql:5.5"]
async fn a_read_only_profile_connects_to_mysql_without_read_only_transaction_support() {
    let image = GenericImage::new("mysql", "5.5")
        .with_exposed_port(3306.tcp())
        .with_wait_for(WaitFor::message_on_stderr("port: 3306"))
        .with_env_var("MYSQL_ROOT_PASSWORD", "bookie_test")
        .with_env_var("MYSQL_DATABASE", "test");
    let container = image
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .expect("start the MySQL 5.5 compatibility fixture");
    let options = ConnectOptions {
        host: container.get_host().await.expect("container host").to_string(),
        port: container.get_host_port_ipv4(3306).await.expect("mapped MySQL port"),
        database: "test".into(),
        username: "root".into(),
        password: secrecy::SecretString::new("bookie_test".to_string().into()),
        tls: TlsConfig::disabled(),
        read_only: true,
        ..Default::default()
    };
    let raw_options = MySqlConnectOptions::new()
        .host(&options.host)
        .port(options.port)
        .database(&options.database)
        .username(&options.username)
        .password(options.password.expose_secret())
        .ssl_mode(MySqlSslMode::Disabled);
    let mut raw = sqlx::MySqlConnection::connect_with(&raw_options)
        .await
        .expect("connect to the compatibility fixture directly");
    let error = sqlx::query("SET SESSION TRANSACTION READ ONLY")
        .execute(&mut raw)
        .await
        .expect_err("MySQL 5.5 does not support read-only transaction setup");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|error| error.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>())
            .map(sqlx::mysql::MySqlDatabaseError::number),
        Some(1193),
    );
    raw.close().await.expect("close raw connection");

    let connection = MysqlDriver
        .connect(options)
        .await
        .expect("read-only profile should use PolicyGuard when MySQL lacks transaction mode support");
    let result = connection.query("SELECT VERSION()").await.expect("read server version");
    assert!(
        matches!(result.rows.first().and_then(|row| row.first()), Some(Value::Text(version)) if version.starts_with("5.5"))
    );
    connection.close().await.expect("close connection");
}
