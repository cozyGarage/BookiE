#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use drivers_mysql::MysqlDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver};
use testcontainers::ImageExt;
use testcontainers::core::Mount;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_accepts_a_query_through_a_local_socket_file() {
    use std::os::unix::fs::PermissionsExt;

    let socket_directory = tempfile::tempdir().expect("socket directory");
    std::fs::set_permissions(socket_directory.path(), std::fs::Permissions::from_mode(0o777))
        .expect("make socket directory accessible to the container");
    let image = Mysql::default()
        .with_env_var("MYSQL_ROOT_PASSWORD", "tablepro_test")
        .with_cmd(["--default-authentication-plugin=mysql_native_password"])
        .with_mount(Mount::bind_mount(
            socket_directory.path().to_string_lossy().to_string(),
            "/var/run/mysqld",
        ));
    let image = match std::env::var("TABLEPRO_TEST_RUN_ID") {
        Ok(run_id) => image.with_label("com.tablepro.test-run", run_id),
        Err(_) => image,
    };
    let container = image
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .expect("start mysql container");
    let options = ConnectOptions {
        local_socket_path: Some(socket_directory.path().join("mysqld.sock")),
        database: "mysql".into(),
        username: "root".into(),
        password: secrecy::SecretString::new("tablepro_test".to_string().into()),
        ..Default::default()
    };

    let connection = MysqlDriver
        .connect(options)
        .await
        .expect("connect through local socket");
    let result = connection.query("SELECT 1").await.expect("query through local socket");
    assert_eq!(result.rows.len(), 1);
    connection.close().await.expect("close local socket connection");
    container.stop().await.expect("stop mysql container");
}
