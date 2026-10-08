use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use tablepro_core::ConnectOptions;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::sync::Mutex;

static POSTGRES_SERVER: OnceLock<Mutex<Option<ContainerAsync<Postgres>>>> = OnceLock::new();
static NEXT_POSTGRES_DATABASE: AtomicU64 = AtomicU64::new(0);

pub(super) async fn start_pg() -> ((), ConnectOptions) {
    let server = POSTGRES_SERVER.get_or_init(|| Mutex::new(None));
    let mut shared = server.lock().await;
    if shared.is_none() {
        let container = start_pg_container().await;
        super::container_cleanup::register(container.id());
        *shared = Some(container);
    }
    let mut options = pg_options(shared.as_ref().unwrap()).await;
    drop(shared);

    let database = format!(
        "bookie_test_{}_{}",
        std::process::id(),
        NEXT_POSTGRES_DATABASE.fetch_add(1, Ordering::Relaxed)
    );
    let admin = super::connect(options.clone()).await;
    admin
        .execute(&format!("CREATE DATABASE {database} TEMPLATE template0"))
        .await
        .expect("create isolated PostgreSQL test database");
    admin.close().await.expect("close PostgreSQL admin connection");
    options.database = database;
    ((), options)
}

pub(super) async fn start_pg_dedicated() -> (ContainerAsync<Postgres>, ConnectOptions) {
    let container = start_pg_container().await;
    let options = pg_options(&container).await;
    (container, options)
}

async fn start_pg_container() -> ContainerAsync<Postgres> {
    Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("start postgres container")
}

async fn pg_options(container: &ContainerAsync<Postgres>) -> ConnectOptions {
    ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: tablepro_core::TlsConfig::disabled(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn test_databases_are_isolated_on_one_postgres_server() {
    let (_, first_options) = start_pg().await;
    let (_, second_options) = start_pg().await;
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
