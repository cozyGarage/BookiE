#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../shared/tests/conformance.rs"]
mod conformance;

use std::path::Path;
use std::sync::OnceLock;

use bookie_core::{ConnectOptions, DatabaseDriver, TlsConfig};
use drivers_mysql::MysqlDriver;
use testcontainers::ImageExt;
use testcontainers_modules::mysql::Mysql as MysqlImage;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

static SERVER: OnceLock<ConnectOptions> = OnceLock::new();

struct MySql;

fn database_for(path: &Path) -> String {
    let directory = path.parent().and_then(Path::file_name).unwrap().to_string_lossy();
    let tail: String = directory.chars().filter(char::is_ascii_alphanumeric).collect();
    format!("conf_{}", tail.to_ascii_lowercase())
}

impl conformance::ConformanceEngine for MySql {
    type Driver = MysqlDriver;

    fn driver() -> MysqlDriver {
        MysqlDriver
    }

    const FILE_BASED: bool = false;

    async fn prepare(path: &Path) {
        let admin = MysqlDriver
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
async fn conformance_suite_on_a_real_mysql_server() {
    let container = MysqlImage::default()
        .with_env_var("MYSQL_ROOT_PASSWORD", "bookie_test")
        .with_cmd(["--default-authentication-plugin=mysql_native_password"])
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .expect("start mysql container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(3306).await.expect("port"),
        database: "mysql".into(),
        username: "root".into(),
        password: secrecy::SecretString::new("bookie_test".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    SERVER.set(options).expect("one server per run");
    conformance::run_all::<MySql>().await;
}
