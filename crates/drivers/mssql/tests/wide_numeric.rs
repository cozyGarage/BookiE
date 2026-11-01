#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use bookie_core::DatabaseDriver;
use drivers_mssql::MssqlDriver;

#[path = "support/mssql_startup.rs"]
mod mssql_startup;
#[path = "support/wide_numeric.rs"]
mod wide_numeric;

#[tokio::test]
#[ignore = "requires docker"]
async fn sql_server_numeric_values_outside_rust_decimal_round_trip_as_exact_text() {
    let (_container, options) = mssql_startup::start_mssql(false).await;
    let connection = MssqlDriver.connect(options).await.expect("connect to SQL Server");
    wide_numeric::assert_contract(connection.as_ref()).await;
}
