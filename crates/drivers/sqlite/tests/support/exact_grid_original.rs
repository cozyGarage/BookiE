use tablepro_core::{ConnectOptions, DatabaseDriver, Value};
#[path = "../../../shared/exact_grid_original.rs"]
mod contract;

#[tokio::test]
async fn exact_grid_original_detects_concurrent_text_changes() {
    let directory = tempfile::tempdir().unwrap();
    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: directory.path().join("grid.db").to_string_lossy().into(),
            ..Default::default()
        })
        .await
        .unwrap();
    for (kind, old, concurrent, search_matches) in [
        ("TEXT COLLATE NOCASE", "Ada", "ADA", Some(2)),
        ("TEXT COLLATE NOCASE", "Ada\0x", "Ada\0y", Some(2)),
        ("TEXT COLLATE RTRIM", "Ada", "Ada ", Some(2)),
        ("TEXT", "café", "cafe", Some(1)),
    ] {
        connection
            .execute(&format!("CREATE TABLE exact_grid (id INT PRIMARY KEY, v {kind})"))
            .await
            .unwrap();
        contract::assert_conflict(
            connection.as_ref(),
            "sqlite",
            Value::Text(old.into()),
            Value::Text(concurrent.into()),
            search_matches,
        )
        .await;
        connection.execute("DROP TABLE exact_grid").await.unwrap();
    }
    contract::assert_key_conflict(connection.as_ref(), "sqlite", "TEXT COLLATE NOCASE").await;
    connection
        .execute("CREATE TABLE exact_grid (id INT PRIMARY KEY, v BLOB)")
        .await
        .unwrap();
    contract::assert_conflict(
        connection.as_ref(),
        "sqlite",
        Value::Bytes(vec![65]),
        Value::Bytes(vec![65, 0]),
        None,
    )
    .await;
    connection.execute("DROP TABLE exact_grid").await.unwrap();
}
