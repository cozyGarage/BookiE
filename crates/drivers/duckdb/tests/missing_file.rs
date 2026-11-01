#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use drivers_duckdb::DuckdbDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver};
use tempfile::TempDir;

#[tokio::test]
async fn connecting_to_a_missing_duckdb_file_path_fails() {
    let dir = TempDir::new().unwrap();
    let missing_path = dir.path().join("does_not_exist.duckdb");
    let options = ConnectOptions {
        database: missing_path.to_string_lossy().to_string(),
        ..Default::default()
    };
    let result = DuckdbDriver.connect(options).await;
    assert!(result.is_err());
    assert!(!missing_path.exists());
}
