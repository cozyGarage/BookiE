#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use drivers_mssql::MssqlDriver;
use secrecy::SecretString;
use tablepro_core::{ConnectOptions, DatabaseDriver};
use testcontainers::ContainerAsync;
use testcontainers_modules::mssql_server::MssqlServer;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[path = "support/wide_numeric.rs"]
mod wide_numeric;

#[tokio::test]
#[ignore = "requires docker"]
async fn sql_server_numeric_values_outside_rust_decimal_round_trip_as_exact_text() {
    let (_container, connection) = start_mssql().await;
    wide_numeric::assert_contract(connection.as_ref()).await;
}

async fn start_mssql() -> (ContainerAsync<MssqlServer>, Box<dyn tablepro_core::Connection>) {
    let container = MssqlServer::default()
        .with_accept_eula()
        .start()
        .await
        .expect("start MSSQL container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(1433).await.expect("port"),
        database: "master".into(),
        username: "sa".into(),
        password: SecretString::new(MssqlServer::DEFAULT_SA_PASSWORD.to_string().into()),
        tls: tablepro_core::TlsConfig::disabled(),
        ..Default::default()
    };
    let connection = MssqlDriver.connect(options).await.expect("connect to MSSQL");
    (container, connection)
}
