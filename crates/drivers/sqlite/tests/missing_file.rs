#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use drivers_sqlite::SqliteDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver};
use tempfile::TempDir;

#[tokio::test]
async fn connecting_to_a_missing_sqlite_file_path_fails() {
    let dir = TempDir::new().unwrap();
    let missing_path = dir.path().join("does_not_exist.db");
    let options = ConnectOptions {
        database: missing_path.to_string_lossy().to_string(),
        ..Default::default()
    };
    let result = SqliteDriver.connect(options).await;
    assert!(result.is_err());
    assert!(!missing_path.exists());
}
