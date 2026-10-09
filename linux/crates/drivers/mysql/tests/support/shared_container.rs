use std::sync::{LazyLock, Mutex as StdMutex};

use drivers_mysql::MysqlDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver, TlsConfig, Value};
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::sync::{Mutex, MutexGuard, OnceCell};

struct Server {
    options: ConnectOptions,
    sql_mode: String,
}

pub(crate) struct TestDatabase {
    _container: Option<ContainerAsync<Mysql>>,
    _test_lock: Option<MutexGuard<'static, ()>>,
}

static SERVER: OnceCell<Server> = OnceCell::const_new();
static CONTAINER: StdMutex<Option<ContainerAsync<Mysql>>> = StdMutex::new(None);
static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(crate) async fn start_mysql() -> (TestDatabase, ConnectOptions) {
    if can_reuse_server(
        &std::env::args().skip(1).collect::<Vec<_>>(),
        std::env::var_os("TABLEPRO_TEST_RUN_ID").is_some(),
    ) {
        let test_lock = TEST_LOCK.lock().await;
        let server = SERVER.get_or_init(start_shared_server).await;
        reset_server(server).await;
        return (
            TestDatabase {
                _container: None,
                _test_lock: Some(test_lock),
            },
            test_options(&server.options),
        );
    }
    let (container, server) = create_server().await;
    (
        TestDatabase {
            _container: Some(container),
            _test_lock: None,
        },
        test_options(&server.options),
    )
}

fn test_options(options: &ConnectOptions) -> ConnectOptions {
    ConnectOptions {
        database: "test".into(),
        ..options.clone()
    }
}

fn can_reuse_server(args: &[String], runner_cleans_container: bool) -> bool {
    let serial = args.iter().any(|arg| arg == "--test-threads=1")
        || args
            .windows(2)
            .any(|pair| pair[0] == "--test-threads" && pair[1] == "1");
    let exact = args.iter().any(|arg| arg == "--exact");
    let selection = args.iter().find(|arg| !arg.starts_with('-') && *arg != "1");
    runner_cleans_container && serial && !exact && selection.is_none_or(|arg| arg == "value_contract")
}

async fn start_shared_server() -> Server {
    let (container, server) = create_server().await;
    *CONTAINER.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(container);
    server
}

async fn create_server() -> (ContainerAsync<Mysql>, Server) {
    let image = Mysql::default()
        .with_env_var("MYSQL_ROOT_PASSWORD", "tablepro_test")
        .with_cmd(["--default-authentication-plugin=mysql_native_password"]);
    let image = match std::env::var("TABLEPRO_TEST_RUN_ID") {
        Ok(run_id) => image.with_label("com.tablepro.test-run", run_id),
        Err(_) => image,
    };
    let container = image.start().await.expect("start mysql container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(3306).await.expect("port"),
        database: "mysql".into(),
        username: "root".into(),
        password: secrecy::SecretString::new("tablepro_test".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let connection = MysqlDriver.connect(options.clone()).await.expect("connect to mysql");
    let result = connection
        .query("SELECT @@GLOBAL.sql_mode")
        .await
        .expect("read mysql sql_mode");
    let Some(Value::Text(sql_mode)) = result.rows.first().and_then(|row| row.first()) else {
        panic!("unexpected mysql sql_mode result: {:?}", result.rows);
    };
    connection.close().await.expect("close mysql setup connection");
    (
        container,
        Server {
            options,
            sql_mode: sql_mode.clone(),
        },
    )
}

async fn reset_server(server: &Server) {
    let connection = MysqlDriver
        .connect(server.options.clone())
        .await
        .expect("connect to shared mysql");
    connection
        .execute("DROP DATABASE IF EXISTS test")
        .await
        .expect("drop prior test database");
    connection
        .execute("CREATE DATABASE test")
        .await
        .expect("create fresh test database");
    connection
        .execute("DROP DATABASE IF EXISTS alpha_db")
        .await
        .expect("remove prior database-list fixture");
    let sql_mode = format!("SET GLOBAL sql_mode = '{}'", server.sql_mode.replace('\'', "''"));
    connection
        .execute(&sql_mode)
        .await
        .expect("reset mysql global sql_mode");
    connection.close().await.expect("close mysql setup connection");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_mysql_fixtures_reset_the_database_on_one_server() {
    {
        let (_fixture, options) = start_mysql().await;
        let connection = MysqlDriver.connect(options).await.unwrap();
        connection
            .execute("CREATE TABLE fixture_isolation (value INT)")
            .await
            .unwrap();
    }
    let (_fixture, options) = start_mysql().await;
    let connection = MysqlDriver.connect(options).await.unwrap();
    assert!(
        connection
            .query("SHOW TABLES LIKE 'fixture_isolation'")
            .await
            .unwrap()
            .rows
            .is_empty()
    );
}

#[test]
fn shared_server_requires_a_serial_complete_selection() {
    assert!(can_reuse_server(
        &["--include-ignored".into(), "--test-threads=1".into()],
        true
    ));
    assert!(can_reuse_server(
        &[
            "value_contract".into(),
            "--include-ignored".into(),
            "--test-threads=1".into()
        ],
        true
    ));
    assert!(!can_reuse_server(
        &["--include-ignored".into(), "--test-threads=1".into()],
        false
    ));
    assert!(!can_reuse_server(
        &["value_contract".into(), "--include-ignored".into()],
        true
    ));
    assert!(!can_reuse_server(
        &["value_contract".into(), "--exact".into(), "--test-threads=1".into()],
        true
    ));
    assert!(!can_reuse_server(
        &["specific_test".into(), "--test-threads=1".into()],
        true
    ));
}
