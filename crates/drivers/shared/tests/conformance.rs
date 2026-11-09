use std::path::Path;

use bookie_core::{ConnectOptions, Connection, DatabaseDriver, Value};
use tempfile::TempDir;

#[allow(async_fn_in_trait)]
pub trait ConformanceEngine {
    type Driver: DatabaseDriver;
    fn driver() -> Self::Driver;
    const FILE_BASED: bool;
    /// Make a fresh, empty database for one case; `path` is unique per case.
    async fn prepare(_path: &Path) {}
    /// Connection options for the database `prepare` made for `path`.
    fn options(path: &Path, read_only: bool) -> ConnectOptions {
        ConnectOptions {
            database: path.to_string_lossy().into_owned(),
            read_only,
            ..ConnectOptions::default()
        }
    }
}

const SCHEMA: &str = "CREATE TABLE conf_items (id BIGINT PRIMARY KEY, label VARCHAR(20), n BIGINT)";
const SEED: [&str; 3] = [
    "INSERT INTO conf_items VALUES (1, 'one', 9007199254740993)",
    "INSERT INTO conf_items VALUES (2, NULL, NULL)",
    "INSERT INTO conf_items VALUES (3, '', -5)",
];

async fn open<E: ConformanceEngine>(path: &Path, read_only: bool) -> Box<dyn Connection> {
    E::driver().connect(E::options(path, read_only)).await.expect("connect")
}

pub async fn seeded<E: ConformanceEngine>() -> (TempDir, std::path::PathBuf) {
    let directory = TempDir::new().expect("temp dir");
    let path = directory.path().join("conformance.db");
    E::prepare(&path).await;
    let writer = open::<E>(&path, false).await;
    writer.execute(SCHEMA).await.expect("create table");
    for statement in SEED {
        writer.execute(statement).await.expect("seed row");
    }
    writer.close().await.expect("close writer");
    (directory, path)
}

pub async fn declared_capabilities_match_the_suite<E: ConformanceEngine>() {
    let capabilities = E::driver().capabilities();
    assert_eq!(capabilities.is_file_based, E::FILE_BASED);
    assert!(!E::driver().id().is_empty());
}

pub async fn read_only_connections_refuse_every_write_and_keep_the_rows<E: ConformanceEngine>() {
    let (_directory, path) = seeded::<E>().await;
    let reader = open::<E>(&path, true).await;
    for statement in [
        "INSERT INTO conf_items VALUES (4, 'four', 4)",
        "UPDATE conf_items SET label = 'x'",
        "DELETE FROM conf_items",
        "DROP TABLE conf_items",
    ] {
        assert!(reader.execute(statement).await.is_err(), "{statement} must be refused");
    }
    let batch = [("DELETE FROM conf_items".to_string(), Vec::new())];
    assert!(reader.execute_in_transaction(&batch).await.is_err());
    let count = reader.query("SELECT count(*) FROM conf_items").await.expect("count");
    assert_eq!(count.rows.iter().next().map(|row| row[0].clone()), Some(Value::Int(3)));
}

pub async fn null_empty_text_and_wide_integers_round_trip_exactly<E: ConformanceEngine>() {
    let (_directory, path) = seeded::<E>().await;
    let connection = open::<E>(&path, true).await;
    let result = connection
        .query("SELECT id, label, n FROM conf_items ORDER BY id")
        .await
        .expect("select");
    let rows: Vec<Vec<Value>> = result.rows.iter().map(|row| row.to_vec()).collect();
    assert_eq!(
        rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("one".into()),
                Value::Int(9_007_199_254_740_993)
            ],
            vec![Value::Int(2), Value::Null, Value::Null],
            vec![Value::Int(3), Value::Text(String::new()), Value::Int(-5)],
        ]
    );
}

pub async fn browse_pages_cover_every_row_once_and_the_last_page_is_short<E: ConformanceEngine>() {
    let (_directory, path) = seeded::<E>().await;
    let connection = open::<E>(&path, true).await;
    let mut seen = Vec::new();
    for (offset, expected_len) in [(0_u64, 2_usize), (2, 1), (3, 0)] {
        let page = connection
            .fetch_rows(None, "conf_items", offset, 2)
            .await
            .expect("page");
        let ids: Vec<Value> = page.rows.iter().map(|row| row[0].clone()).collect();
        assert_eq!(ids.len(), expected_len, "offset {offset}");
        seen.extend(ids);
    }
    seen.sort_by_key(|id| match id {
        Value::Int(value) => *value,
        _ => i64::MAX,
    });
    assert_eq!(seen, vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
}

pub async fn the_catalog_names_the_table_and_describes_its_columns<E: ConformanceEngine>() {
    let (_directory, path) = seeded::<E>().await;
    let connection = open::<E>(&path, true).await;
    let tables = connection.list_tables().await.expect("tables");
    assert!(tables.iter().any(|table| table.name == "conf_items"), "{tables:?}");
    let columns = connection.fetch_columns(None, "conf_items").await.expect("columns");
    let names: Vec<&str> = columns.iter().map(|column| column.name.as_str()).collect();
    assert_eq!(names, ["id", "label", "n"]);
    assert!(columns[0].primary_key && !columns[1].primary_key);
    assert!(columns[1].nullable);
}

/// One runtime and one server for network engines, which cannot afford a container per case.
#[allow(dead_code)] // used by network engines only
pub async fn run_all<E: ConformanceEngine>() {
    declared_capabilities_match_the_suite::<E>().await;
    read_only_connections_refuse_every_write_and_keep_the_rows::<E>().await;
    null_empty_text_and_wide_integers_round_trip_exactly::<E>().await;
    browse_pages_cover_every_row_once_and_the_last_page_is_short::<E>().await;
    the_catalog_names_the_table_and_describes_its_columns::<E>().await;
}

#[macro_export]
macro_rules! conformance_suite {
    ($engine:ty) => {
        #[tokio::test]
        async fn conformance_declared_capabilities_match_the_suite() {
            conformance::declared_capabilities_match_the_suite::<$engine>().await;
        }
        #[tokio::test]
        async fn conformance_read_only_connections_refuse_every_write_and_keep_the_rows() {
            conformance::read_only_connections_refuse_every_write_and_keep_the_rows::<$engine>().await;
        }
        #[tokio::test]
        async fn conformance_null_empty_text_and_wide_integers_round_trip_exactly() {
            conformance::null_empty_text_and_wide_integers_round_trip_exactly::<$engine>().await;
        }
        #[tokio::test]
        async fn conformance_browse_pages_cover_every_row_once_and_the_last_page_is_short() {
            conformance::browse_pages_cover_every_row_once_and_the_last_page_is_short::<$engine>().await;
        }
        #[tokio::test]
        async fn conformance_the_catalog_names_the_table_and_describes_its_columns() {
            conformance::the_catalog_names_the_table_and_describes_its_columns::<$engine>().await;
        }
    };
}
