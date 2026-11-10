#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../shared/tests/conformance.rs"]
mod conformance;

use std::path::Path;
use std::sync::OnceLock;

use bookie_core::{ConnectOptions, DatabaseDriver, TlsConfig};
use drivers_mssql::MssqlDriver;
use secrecy::SecretString;
use testcontainers::ImageExt;
use testcontainers::core::Mount;
use testcontainers_modules::mssql_server::MssqlServer;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

static SERVER: OnceLock<ConnectOptions> = OnceLock::new();

struct Mssql;

fn database_for(path: &Path) -> String {
    let directory = path.parent().and_then(Path::file_name).unwrap().to_string_lossy();
    let tail: String = directory.chars().filter(char::is_ascii_alphanumeric).collect();
    format!("conf_{}", tail.to_ascii_lowercase())
}

impl conformance::ConformanceEngine for Mssql {
    type Driver = MssqlDriver;

    fn driver() -> MssqlDriver {
        MssqlDriver
    }

    const FILE_BASED: bool = false;
    const DRIVER_ENFORCES_READ_ONLY: bool = false;

    async fn prepare(path: &Path) {
        let admin = MssqlDriver
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
async fn conformance_suite_on_a_real_sql_server() {
    let container = MssqlServer::default()
        .with_accept_eula()
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .with_mount(Mount::tmpfs_mount("/var/opt/mssql").with_mode(0o1777))
        .start()
        .await
        .expect("start MSSQL container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(1433).await.expect("port"),
        database: "master".into(),
        username: "sa".into(),
        password: SecretString::new(MssqlServer::DEFAULT_SA_PASSWORD.to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let mut ready = false;
    for _ in 0..120 {
        if let Ok(connection) = MssqlDriver.connect(options.clone()).await
            && connection.query("SELECT 1").await.is_ok()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    assert!(ready, "SQL Server never answered SELECT 1");
    SERVER.set(options).expect("one server per run");
    conformance::run_all::<Mssql>().await;
}
