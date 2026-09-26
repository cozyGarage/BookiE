#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use drivers_duckdb::DuckdbDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver};

#[path = "../../../core/tests/support/value_contract.rs"]
mod value_contract;

#[tokio::test]
async fn value_contract_preserves_scalar_boundaries_through_parameters_and_exports() {
    let connection = DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    value_contract::assert_scalar_contract(connection.as_ref(), "duckdb").await;
}
