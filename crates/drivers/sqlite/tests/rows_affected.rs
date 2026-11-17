#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use bookie_core::{ConnectOptions, DatabaseDriver, OperationControl};
use drivers_sqlite::SqliteDriver;
use tempfile::TempDir;

async fn batch(sql: &str, connection: &dyn bookie_core::Connection) -> Option<u64> {
    connection
        .query_result_sets_controlled(
            sql,
            &[],
            &OperationControl::with_timeout(std::time::Duration::from_secs(30)),
        )
        .await
        .unwrap()
        .rows_affected
}

#[tokio::test]
async fn a_write_reports_how_many_rows_it_changed_and_a_select_reports_none() {
    let directory = TempDir::new().unwrap();
    std::fs::File::create(directory.path().join("w.db")).unwrap();
    let connection = SqliteDriver
        .connect(ConnectOptions {
            database: directory.path().join("w.db").to_string_lossy().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let connection = connection.as_ref();

    assert_eq!(
        batch("CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT)", connection).await,
        Some(0)
    );
    assert_eq!(
        batch("INSERT INTO notes(body) VALUES ('a'),('b'),('c')", connection).await,
        Some(3)
    );
    assert_eq!(
        batch("UPDATE notes SET body = 'z' WHERE id <= 2", connection).await,
        Some(2)
    );
    assert_eq!(batch("DELETE FROM notes", connection).await, Some(3));
    assert_eq!(batch("SELECT * FROM notes", connection).await, None);
}
