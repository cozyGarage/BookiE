#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use drivers_sqlite::SqliteDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver};
use tempfile::TempDir;

#[tokio::test]
async fn tables_named_like_the_internal_prefix_are_listed() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("names.db");
    let _ = std::fs::File::create(&db_path);
    let options = ConnectOptions {
        database: db_path.to_string_lossy().to_string(),
        ..Default::default()
    };
    let conn = SqliteDriver.connect(options).await.unwrap();
    for name in ["sqlite3_results", "sqliteapp", "plain"] {
        conn.execute(&format!("CREATE TABLE {name} (id INTEGER PRIMARY KEY AUTOINCREMENT)"))
            .await
            .unwrap();
    }
    let names: Vec<_> = conn.list_tables().await.unwrap().into_iter().map(|t| t.name).collect();
    assert_eq!(names, ["plain", "sqlite3_results", "sqliteapp"]);
}
