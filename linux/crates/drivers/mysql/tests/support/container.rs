use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use tablepro_core::ConnectOptions;
use testcontainers::ContainerAsync;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::{GenericImage, ImageExt};
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::sync::Mutex;

static MYSQL_SERVER: OnceLock<Mutex<Option<ContainerAsync<Mysql>>>> = OnceLock::new();
static MARIADB_SERVER: OnceLock<Mutex<Option<ContainerAsync<GenericImage>>>> = OnceLock::new();
static NEXT_MYSQL_DATABASE: AtomicU64 = AtomicU64::new(0);
static NEXT_MARIADB_DATABASE: AtomicU64 = AtomicU64::new(0);

pub(super) async fn start_mysql() -> ((), ConnectOptions) {
    let options = isolate_mysql_database(shared_mysql().await, &NEXT_MYSQL_DATABASE).await;
    ((), options)
}

pub(super) async fn start_mysql_dedicated() -> (ContainerAsync<Mysql>, ConnectOptions) {
    let container = start_mysql_container().await;
    let options = mysql_options(&container).await;
    (container, options)
}

pub(super) async fn start_mariadb() -> ((), ConnectOptions) {
    let options = isolate_mysql_database(shared_mariadb().await, &NEXT_MARIADB_DATABASE).await;
    ((), options)
}

pub(super) async fn start_mariadb_dedicated() -> (ContainerAsync<GenericImage>, ConnectOptions) {
    let container = start_mariadb_container().await;
    let options = mariadb_options(&container).await;
    (container, options)
}

async fn shared_mysql() -> ConnectOptions {
    let server = MYSQL_SERVER.get_or_init(|| Mutex::new(None));
    let mut shared = server.lock().await;
    if shared.is_none() {
        let container = start_mysql_container().await;
        super::container_cleanup::register(container.id());
        *shared = Some(container);
    }
    mysql_options(shared.as_ref().unwrap()).await
}

async fn shared_mariadb() -> ConnectOptions {
    let server = MARIADB_SERVER.get_or_init(|| Mutex::new(None));
    let mut shared = server.lock().await;
    if shared.is_none() {
        let container = start_mariadb_container().await;
        super::container_cleanup::register(container.id());
        *shared = Some(container);
    }
    mariadb_options(shared.as_ref().unwrap()).await
}

async fn start_mysql_container() -> ContainerAsync<Mysql> {
    Mysql::default()
        .with_env_var("MYSQL_ROOT_PASSWORD", "tablepro_test")
        .with_cmd(["--default-authentication-plugin=mysql_native_password"])
        .start()
        .await
        .expect("start mysql container")
}

async fn start_mariadb_container() -> ContainerAsync<GenericImage> {
    GenericImage::new("mariadb", "11")
        .with_exposed_port(3306.tcp())
        .with_wait_for(WaitFor::message_on_stderr("port: 3306"))
        .with_env_var("MARIADB_ROOT_PASSWORD", "tablepro_test")
        .with_env_var("MARIADB_DATABASE", "test")
        .start()
        .await
        .expect("start mariadb container")
}

async fn mysql_options(container: &ContainerAsync<Mysql>) -> ConnectOptions {
    ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(3306).await.expect("port"),
        database: "test".into(),
        username: "root".into(),
        password: secrecy::SecretString::new("tablepro_test".to_string().into()),
        tls: tablepro_core::TlsConfig::disabled(),
        ..Default::default()
    }
}

async fn mariadb_options(container: &ContainerAsync<GenericImage>) -> ConnectOptions {
    ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(3306).await.expect("port"),
        database: "test".into(),
        username: "root".into(),
        password: secrecy::SecretString::new("tablepro_test".to_string().into()),
        tls: tablepro_core::TlsConfig::disabled(),
        ..Default::default()
    }
}

async fn isolate_mysql_database(mut options: ConnectOptions, counter: &AtomicU64) -> ConnectOptions {
    let database = format!(
        "bookie_test_{}_{}",
        std::process::id(),
        counter.fetch_add(1, Ordering::Relaxed)
    );
    let admin = super::connect(options.clone()).await;
    admin
        .execute(&format!("CREATE DATABASE {database}"))
        .await
        .expect("create isolated MySQL test database");
    admin.close().await.expect("close MySQL admin connection");
    options.database = database;
    options
}

#[tokio::test]
#[ignore = "requires docker"]
async fn test_databases_are_isolated_on_one_mysql_server() {
    let (_, first_options) = start_mysql().await;
    let (_, second_options) = start_mysql().await;
    assert_eq!(
        (&first_options.host, first_options.port),
        (&second_options.host, second_options.port)
    );
    assert_ne!(first_options.database, second_options.database);

    super::connect(first_options)
        .await
        .execute("CREATE TABLE isolated_probe (value INT)")
        .await
        .unwrap();
    assert!(
        super::connect(second_options)
            .await
            .query("SELECT * FROM isolated_probe")
            .await
            .is_err()
    );
}
