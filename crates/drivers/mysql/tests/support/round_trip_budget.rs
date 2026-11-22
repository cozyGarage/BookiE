use bookie_core::{ConnectOptions, Connection, DatabaseDriver, OperationControl, TlsConfig, Value};
use drivers_mysql::MysqlDriver;
use testcontainers::ImageExt;
use testcontainers::core::{IntoContainerPort, Mount, WaitFor};
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_util::sync::CancellationToken;

const APP_USER: &str = "bookie_budget";
const APP_PASSWORD: &str = "bookie_budget";
const SIMPLE_SELECT_COMMAND_BUDGET: usize = 10;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_controlled_simple_select_stays_within_its_server_statement_budget() {
    let container = Mysql::default()
        .with_tag(crate::MYSQL_TEST_TAG)
        .with_env_var("MYSQL_ROOT_PASSWORD", "bookie_test")
        .with_env_var("MYSQL_DATABASE", "test")
        .with_cmd([
            "--mysql-native-password=ON",
            "--authentication-policy=mysql_native_password",
            "--general-log=ON",
            "--log-output=TABLE",
            "--innodb-flush-log-at-trx-commit=0",
            "--innodb-doublewrite=0",
            "--sync-binlog=0",
        ])
        .with_mount(testcontainers::core::Mount::tmpfs_mount("/var/lib/mysql"))
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .expect("start MySQL with general query log");
    assert_controlled_select_budget(
        container.get_host().await.expect("container host").to_string(),
        container.get_host_port_ipv4(3306).await.expect("MySQL port"),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_controlled_simple_select_stays_within_its_mariadb_statement_budget() {
    let container = testcontainers::GenericImage::new("mariadb", "11")
        .with_exposed_port(3306.tcp())
        .with_wait_for(WaitFor::message_on_stderr("port: 3306"))
        .with_env_var("MARIADB_ROOT_PASSWORD", "bookie_test")
        .with_env_var("MARIADB_DATABASE", "test")
        .with_cmd([
            "mariadbd",
            "--general-log=ON",
            "--log-output=TABLE",
            "--innodb-flush-log-at-trx-commit=0",
            "--innodb-doublewrite=0",
            "--sync-binlog=0",
        ])
        .with_mount(Mount::tmpfs_mount("/var/lib/mysql"))
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .expect("start MariaDB with general query log");
    assert_controlled_select_budget(
        container.get_host().await.expect("container host").to_string(),
        container.get_host_port_ipv4(3306).await.expect("MariaDB port"),
    )
    .await;
}

async fn assert_controlled_select_budget(host: String, port: u16) {
    let admin_options = ConnectOptions {
        host,
        port,
        database: "mysql".into(),
        username: "root".into(),
        password: secrecy::SecretString::new("bookie_test".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let admin = MysqlDriver
        .connect(admin_options.clone())
        .await
        .expect("connect as fixture admin");
    admin
        .execute("CREATE USER 'bookie_budget'@'%' IDENTIFIED BY 'bookie_budget'")
        .await
        .expect("create isolated app user");
    admin
        .execute("GRANT SELECT ON *.* TO 'bookie_budget'@'%'")
        .await
        .expect("grant only read access to measured user");
    admin
        .execute("SET GLOBAL general_log = OFF")
        .await
        .expect("pause query logging");
    admin
        .execute("TRUNCATE TABLE mysql.general_log")
        .await
        .expect("clear fixture setup statements");
    admin
        .execute("SET GLOBAL general_log = ON")
        .await
        .expect("resume query logging");

    let app: Box<dyn Connection> = MysqlDriver
        .connect(ConnectOptions {
            database: "test".into(),
            username: APP_USER.into(),
            password: secrecy::SecretString::new(APP_PASSWORD.to_string().into()),
            ..admin_options
        })
        .await
        .expect("connect as measured app user");
    let control = OperationControl::new(CancellationToken::new(), None);
    app.query_controlled("SELECT 0 AS warmup", &control)
        .await
        .expect("warm the app pool");
    admin
        .execute("SET GLOBAL general_log = OFF")
        .await
        .expect("pause query logging after warmup");
    admin
        .execute("TRUNCATE TABLE mysql.general_log")
        .await
        .expect("clear warmup statements");
    admin
        .execute("SET GLOBAL general_log = ON")
        .await
        .expect("resume query logging for measured operation");
    assert_eq!(
        app.query_controlled("SELECT 42 AS value", &control)
            .await
            .expect("controlled simple SELECT")
            .rows,
        vec![vec![Value::Int(42)]]
    );

    let observed = admin
        .query(
            "SELECT command_type, argument FROM mysql.general_log \
             WHERE user_host LIKE 'bookie_budget%' ORDER BY event_time, thread_id",
        )
        .await
        .expect("read server query log");
    let statements = observed
        .rows
        .iter()
        .map(|row| format!("{}: {}", log_value(row.first()), log_value(row.get(1))))
        .collect::<Vec<_>>();
    let server_command_count = statements
        .iter()
        .filter(|statement| !statement.starts_with("Connect:") && !statement.starts_with("Quit:"))
        .count();
    assert!(
        server_command_count <= SIMPLE_SELECT_COMMAND_BUDGET,
        "controlled simple SELECT used {server_command_count} server protocol commands (budget {SIMPLE_SELECT_COMMAND_BUDGET}):\n{}",
        statements.join("\n")
    );
    assert!(
        statements.iter().any(|statement| statement.contains("SELECT 42")),
        "the server trace omitted the requested query: {statements:#?}"
    );
}

fn log_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::Text(value)) => value.clone(),
        Some(Value::Bytes(value)) => String::from_utf8_lossy(value).into_owned(),
        Some(value) => format!("{value:?}"),
        None => "<missing>".into(),
    }
}
