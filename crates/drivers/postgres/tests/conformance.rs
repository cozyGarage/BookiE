#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../shared/tests/conformance.rs"]
mod conformance;

use std::path::Path;
use std::sync::OnceLock;

use bookie_core::{ConnectOptions, DatabaseDriver, TlsConfig};
use drivers_postgres::PgDriver;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres as PostgresImage;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

static SERVER: OnceLock<ConnectOptions> = OnceLock::new();

struct Pg;

/// Each case gets its own database, named after its unique temp directory.
fn database_for(path: &Path) -> String {
    let directory = path.parent().and_then(Path::file_name).unwrap().to_string_lossy();
    let tail: String = directory.chars().filter(char::is_ascii_alphanumeric).collect();
    format!("conf_{}", tail.to_ascii_lowercase())
}

impl conformance::ConformanceEngine for Pg {
    type Driver = PgDriver;

    fn driver() -> PgDriver {
        PgDriver
    }

    const FILE_BASED: bool = false;

    async fn prepare(path: &Path) {
        let admin = PgDriver
            .connect(SERVER.get().unwrap().clone())
            .await
            .expect("admin connect");
        let create = format!("CREATE DATABASE {} TEMPLATE template0", database_for(path));
        admin.execute(&create).await.expect("create case database");
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
async fn conformance_suite_on_a_real_postgres_server() {
    let container = PostgresImage::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("start postgres container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    SERVER.set(options).expect("one server per run");
    conformance::run_all::<Pg>().await;
}
