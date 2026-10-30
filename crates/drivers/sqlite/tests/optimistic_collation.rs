#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use drivers_sqlite::SqliteDriver;
use tablepro_core::sql_dialect::build_optimistic_keyed_update;
use tablepro_core::{ColumnInfo, ConnectOptions, DatabaseDriver, DriverError, Value};
use tempfile::TempDir;

fn column(name: &str, data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: !primary_key,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

#[tokio::test]
async fn a_stale_edit_is_refused_when_another_writer_changed_only_the_case() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("collation.db").to_string_lossy().to_string();
    let connection = SqliteDriver
        .connect(ConnectOptions {
            database: path,
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT COLLATE NOCASE)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO people VALUES (1, 'ADA')")
        .await
        .unwrap();

    let columns = [column("id", "INTEGER", true), column("name", "TEXT", false)];
    let (sql, params) = build_optimistic_keyed_update(
        "sqlite",
        None,
        "people",
        &columns,
        &[(1, Value::Text("Ada".into()), Value::Text("stale edit".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    let outcome = connection.execute_in_transaction_checked(&[(sql, params)], &[0]).await;

    assert!(matches!(outcome, Err(DriverError::Transaction { .. })), "{outcome:?}");
    let stored = connection.query("SELECT name FROM people").await.unwrap();
    assert_eq!(stored.rows, vec![vec![Value::Text("ADA".into())]]);
}
