#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../shared/tests/conformance.rs"]
mod conformance;

use std::path::Path;
use std::sync::OnceLock;

use bookie_core::{ConnectOptions, DatabaseDriver, TlsConfig};
use drivers_clickhouse::ClickhouseDriver;
use testcontainers::core::wait::HttpWaitStrategy;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

static SERVER: OnceLock<ConnectOptions> = OnceLock::new();

struct ClickHouse;

fn database_for(path: &Path) -> String {
    let directory = path.parent().and_then(Path::file_name).unwrap().to_string_lossy();
    let tail: String = directory.chars().filter(char::is_ascii_alphanumeric).collect();
    format!("conf_{}", tail.to_ascii_lowercase())
}

impl conformance::ConformanceEngine for ClickHouse {
    type Driver = ClickhouseDriver;

    fn driver() -> ClickhouseDriver {
        ClickhouseDriver
    }

    const FILE_BASED: bool = false;
    const UNIQUE_PRIMARY_KEY: bool = false;

    fn schema() -> &'static str {
        "CREATE TABLE conf_items (id Int64, label Nullable(String), n Nullable(Int64)) ENGINE = MergeTree ORDER BY id"
    }

    async fn prepare(path: &Path) {
        let admin = ClickhouseDriver
            .connect(SERVER.get().unwrap().clone())
            .await
            .expect("admin connect");
        admin
            .execute(&format!("CREATE DATABASE {}", database_for(path)))
            .await
            .expect("create case database");
        admin.close().await.expect("close admin");
    }

    fn options(path: &Path, read_only: bool) -> ConnectOptions {
        ConnectOptions {
            database: database_for(path),
            read_only,
            ..SERVER.get().unwrap().clone()
        }
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn conformance_suite_on_a_real_clickhouse_server() {
    let container = GenericImage::new("clickhouse/clickhouse-server", "24.8")
        .with_exposed_port(8123.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/ping")
                .with_port(8123.tcp())
                .with_expected_status_code(200u16),
        ))
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "bookie")
        .with_env_var("CLICKHOUSE_DB", "default")
        .with_env_var("CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT", "1")
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .expect("start clickhouse container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(8123).await.expect("port"),
        database: "default".into(),
        username: "default".into(),
        password: secrecy::SecretString::new("bookie".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    SERVER.set(options.clone()).expect("one server per run");
    let mut ready = false;
    for _ in 0..120 {
        if let Ok(connection) = ClickhouseDriver.connect(options.clone()).await
            && connection.execute("SELECT 1").await.is_ok()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    assert!(ready, "clickhouse never answered SELECT 1");
    conformance::run_all::<ClickHouse>().await;
}
