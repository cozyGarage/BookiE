#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use bookie_core::{ConnectOptions, DatabaseDriver, Value};
use drivers_sqlite::SqliteDriver;
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

#[tokio::test]
async fn normalization_distinct_table_names_remain_distinct() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("names.db");
    let _ = std::fs::File::create(&db_path);
    let conn = SqliteDriver
        .connect(ConnectOptions {
            database: db_path.to_string_lossy().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let names = ["caf\u{e9}", "cafe\u{301}"];
    for (id, name) in names.iter().enumerate() {
        let quoted = bookie_core::sql_dialect::quote_ident("sqlite", name);
        conn.execute(&format!("CREATE TABLE {quoted} (value INTEGER)"))
            .await
            .unwrap();
        conn.execute(&format!("INSERT INTO {quoted} VALUES ({id})"))
            .await
            .unwrap();
    }

    let listed: Vec<_> = conn
        .list_tables()
        .await
        .unwrap()
        .into_iter()
        .map(|table| table.name)
        .collect();
    assert_eq!(listed.len(), names.len());
    assert!(names.iter().all(|name| listed.iter().any(|listed| listed == name)));
    for (id, name) in names.iter().enumerate() {
        let quoted = bookie_core::sql_dialect::quote_ident("sqlite", name);
        let rows = conn.query(&format!("SELECT value FROM {quoted}")).await.unwrap().rows;
        assert_eq!(rows, vec![vec![Value::Int(id as i64)]]);
        let columns = conn.fetch_columns(None, name).await.unwrap();
        assert_eq!(columns[0].name, "value");
    }
}
