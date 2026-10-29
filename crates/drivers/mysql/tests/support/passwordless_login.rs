#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use drivers_mysql::MysqlDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver};
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
#[ignore = "requires docker"]
async fn an_account_without_a_password_signs_in_with_an_empty_password() {
    let container = Mysql::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(3306).await.unwrap();
    let options = ConnectOptions {
        host: "127.0.0.1".into(),
        port,
        username: "root".into(),
        database: "test".into(),
        ..Default::default()
    };

    let connection = MysqlDriver
        .connect(options)
        .await
        .expect("a passwordless account signs in");

    assert_eq!(connection.query("SELECT 1").await.unwrap().rows.len(), 1);
}
