#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use bookie_core::{ConnectOptions, DatabaseDriver, DriverError};
use drivers_duckdb::DuckdbDriver;
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

#[tokio::test]
async fn a_missing_duckdb_file_is_reported_as_a_missing_database_file() {
    let dir = TempDir::new().unwrap();
    let missing_path = dir.path().join("moved.duckdb").to_string_lossy().to_string();
    for read_only in [false, true] {
        let options = ConnectOptions {
            database: missing_path.clone(),
            read_only,
            ..Default::default()
        };
        let error = DuckdbDriver.connect(options).await.err().unwrap();
        assert!(
            matches!(&error, DriverError::DatabaseFileNotFound(path) if *path == missing_path),
            "read_only={read_only}: {error:?}"
        );
    }
}

#[tokio::test]
async fn a_missing_flat_file_is_reported_as_a_missing_database_file() {
    let dir = TempDir::new().unwrap();
    let missing_path = dir.path().join("moved.csv").to_string_lossy().to_string();
    for read_only in [false, true] {
        let options = ConnectOptions {
            database: missing_path.clone(),
            read_only,
            ..Default::default()
        };
        let error = DuckdbDriver.connect(options).await.err().unwrap();
        assert!(
            matches!(&error, DriverError::DatabaseFileNotFound(path) if *path == missing_path),
            "read_only={read_only}: {error:?}"
        );
    }
}

#[tokio::test]
async fn an_initialised_new_file_opens_keeps_data_and_stays_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("new.duckdb");
    std::fs::write(&path, b"").unwrap();
    DuckdbDriver.initialize_new_database_file(&path).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(leftovers.len(), 1);

    let options = ConnectOptions {
        database: path.to_string_lossy().to_string(),
        ..Default::default()
    };
    let conn = DuckdbDriver.connect(options.clone()).await.unwrap();
    conn.execute("CREATE TABLE t (id INTEGER)").await.unwrap();
    conn.execute("INSERT INTO t VALUES (7)").await.unwrap();
    conn.close().await.unwrap();

    let reopened = DuckdbDriver.connect(options).await.unwrap();
    assert_eq!(reopened.query("SELECT id FROM t").await.unwrap().rows.len(), 1);
}
