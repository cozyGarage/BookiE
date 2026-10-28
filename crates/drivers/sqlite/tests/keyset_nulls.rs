#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use drivers_sqlite::SqliteDriver;
use tablepro_core::{ConnectOptions, Connection, DatabaseDriver, Value};
use tablepro_core::{keyset_before_where_clause, keyset_order_by, keyset_order_by_before, keyset_where_clause};
use tempfile::TempDir;

async fn seeded(directory: &TempDir) -> Box<dyn Connection> {
    let connection = SqliteDriver
        .connect(ConnectOptions {
            database: directory.path().join("keyset.db").to_string_lossy().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE t (a INTEGER, b TEXT, PRIMARY KEY (a, b))")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO t VALUES (NULL, 'q'), (1, NULL), (1, 'x'), (2, NULL), (2, 'y'), (3, 'z')")
        .await
        .unwrap();
    connection
}

async fn walk(connection: &dyn Connection, backwards: bool) -> Vec<Vec<Value>> {
    let columns = ["a", "b"];
    let mut seen: Vec<Vec<Value>> = Vec::new();
    loop {
        let (predicate, params) = match (seen.last(), backwards) {
            (None, _) => (String::from("1 = 1"), Vec::new()),
            (Some(last), false) => keyset_where_clause("sqlite", &columns, last, 0).unwrap(),
            (Some(last), true) => keyset_before_where_clause("sqlite", &columns, last, 0).unwrap(),
        };
        let order = if backwards {
            keyset_order_by_before("sqlite", &columns)
        } else {
            keyset_order_by("sqlite", &columns)
        };
        let page = connection
            .query_params(&format!("SELECT a, b FROM t WHERE {predicate}{order} LIMIT 1"), &params)
            .await
            .unwrap();
        let Some(row) = page.rows.into_iter().next() else {
            return seen;
        };
        seen.push(row);
    }
}

#[tokio::test]
async fn keyset_paging_reaches_every_row_when_key_components_are_null() {
    let directory = TempDir::new().unwrap();
    let connection = seeded(&directory).await;
    let all = connection.query("SELECT a, b FROM t ORDER BY a, b").await.unwrap().rows;

    let forwards = walk(connection.as_ref(), false).await;
    let mut backwards = walk(connection.as_ref(), true).await;
    backwards.reverse();

    assert_eq!(forwards, all);
    assert_eq!(backwards, all);
}
