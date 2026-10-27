#[test]
fn a_float_keeps_the_digits_duckdb_shows() {
    assert_eq!(duck_value_ref_to_value(ValueRef::Float(0.1), false), Value::Float(0.1));
}

use super::*;

#[cfg(target_os = "linux")]
#[test]
fn read_only_flat_file_open_does_not_block_if_path_is_replaced_with_a_fifo() {
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("race.csv");
    std::fs::write(&fifo, "id\n7\n").unwrap();
    let expected = std::fs::metadata(&fifo).unwrap();
    std::fs::remove_file(&fifo).unwrap();
    let status = std::process::Command::new("mkfifo").arg(&fifo).status().unwrap();
    assert!(status.success());

    let path = fifo.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    let opener = std::thread::spawn(move || {
        let result = open_selected_file(&path, &expected).map(|_| ());
        let _ = sender.send(result);
    });
    let result = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("opening the selected path must not hang on a FIFO");
    opener.join().unwrap();
    assert!(matches!(result, Err(DriverError::PolicyDenied(_))));
}

async fn in_memory_connection() -> Box<dyn Connection> {
    DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn open_session_is_refused() {
    let conn = in_memory_connection().await;

    match conn.open_session().await {
        Err(DriverError::Unsupported(_)) => {}
        Ok(_) => panic!("duckdb must refuse open_session"),
        Err(other) => panic!("expected Unsupported, got {other:?}"),
    }
}

#[tokio::test]
async fn a_timed_out_query_is_interrupted() {
    let conn = in_memory_connection().await;
    let control = tablepro_core::OperationControl::with_timeout(std::time::Duration::from_millis(5));

    let result = conn
        .query_controlled("SELECT count(*) FROM range(100000000) a, range(2) b", &control)
        .await;

    match result {
        Err(DriverError::TimedOut) => {}
        other => panic!("expected TimedOut, got {other:?}"),
    }
}

#[tokio::test]
async fn cancelling_a_long_query_stops_it_and_frees_the_connection() {
    let conn = in_memory_connection().await;
    let control = tablepro_core::OperationControl::with_timeout(std::time::Duration::from_millis(100));
    let started = std::time::Instant::now();

    let result = conn
        .query_controlled(
            "SELECT count(*) FROM range(1000000000000) a, range(1000000000000) b WHERE a.range + b.range = -1",
            &control,
        )
        .await;

    assert!(matches!(result, Err(DriverError::TimedOut)), "{result:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert!(conn.query("SELECT 1").await.is_ok());
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}

#[tokio::test]
async fn binary_sql_exports_round_trip_null_empty_and_every_byte() {
    let conn = DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    conn.execute("CREATE TABLE binary_exports (id INTEGER, payload BLOB)")
        .await
        .unwrap();
    let columns = conn.fetch_columns(None, "binary_exports").await.unwrap();
    let values = [Value::Null, Value::Bytes(vec![]), Value::Bytes((0u8..=255).collect())];
    for (id, value) in values.iter().enumerate() {
        let sql = tablepro_core::sql_literal::build_insert_literal(
            "duckdb",
            None,
            "binary_exports",
            &columns,
            &[Value::Int(id as i64), value.clone()],
        )
        .unwrap();
        conn.execute(&sql).await.unwrap();
    }
    assert_eq!(
        conn.query("SELECT payload FROM binary_exports ORDER BY id")
            .await
            .unwrap()
            .rows,
        values.into_iter().map(|value| vec![value]).collect::<Vec<_>>()
    );
}

#[test]
fn undecodable_cell_cannot_be_bound_as_null() {
    assert!(matches!(
        values_to_duck_params(&[Value::Undecodable("DECIMAL".into())]),
        Err(DriverError::Unsupported(_))
    ));
}

#[test]
fn intervals_preserve_signed_components_and_refuse_submicrosecond_values() {
    assert_eq!(
        duck_value_ref_to_value(
            ValueRef::Interval {
                months: -2,
                days: 3,
                nanos: -1_000,
            },
            false
        ),
        Value::Text("-2 months 3 days -1 microsecond".into())
    );
    assert!(matches!(
        duck_value_ref_to_value(
            ValueRef::Interval {
                months: 0,
                days: 0,
                nanos: 1,
            },
            false
        ),
        Value::Undecodable(_)
    ));
}

#[test]
fn unsigned_bigint_keeps_the_signed_boundary_exact() {
    assert_eq!(
        duck_value_ref_to_value(ValueRef::UBigInt(i64::MAX as u64), false),
        Value::Int(i64::MAX)
    );
    assert_eq!(
        duck_value_ref_to_value(ValueRef::UBigInt(i64::MAX as u64 + 1), false),
        Value::Text("9223372036854775808".into())
    );
}

#[test]
fn structure_metadata_is_not_declared_without_a_fetch() {
    let d = DuckdbDriver;
    let source = include_str!("lib.rs");
    assert!(!d.supports_index_metadata());
    assert!(!d.supports_foreign_key_metadata());
    assert!(!source.contains(&["async fn ", "fetch_indexes("].concat()));
    assert!(!source.contains(&["async fn ", "fetch_foreign_keys("].concat()));
}
use tempfile::TempDir;

#[test]
fn driver_metadata() {
    let d = DuckdbDriver;
    assert_eq!(d.id(), "duckdb");
    assert_eq!(d.display_name(), "DuckDB");
    assert!(d.is_file_based());
    assert_eq!(d.default_port(), 0);
    assert_eq!(d.default_database(), "");
    assert_eq!(d.default_username(), "");
}

#[test]
fn quote_ident_doubles_quotes() {
    assert_eq!(quote_ident("users"), "\"users\"");
    assert_eq!(quote_ident("a\"b"), "\"a\"\"b\"");
}

#[tokio::test]
async fn connect_create_and_list_tables() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.duckdb");
    let driver = DuckdbDriver;
    let conn = driver
        .connect(ConnectOptions {
            database: path.to_string_lossy().into(),
            ..Default::default()
        })
        .await
        .unwrap();
    conn.execute("CREATE TABLE foo (id INTEGER, name VARCHAR)")
        .await
        .unwrap();
    conn.execute("INSERT INTO foo VALUES (1, 'a'), (2, 'b')").await.unwrap();
    let tables = conn.list_tables().await.unwrap();
    assert!(tables.iter().any(|t| t.name == "foo"));
    let cols = conn.fetch_columns(None, "foo").await.unwrap();
    assert_eq!(cols.len(), 2);
    let rows = conn.fetch_rows(None, "foo", 0, 10).await.unwrap();
    assert_eq!(rows.rows.len(), 2);
}
#[tokio::test]
async fn columns_are_read_for_a_table_whose_name_needs_quoting() {
    let driver = DuckdbDriver;
    let conn = driver.connect(ConnectOptions::default()).await.unwrap();
    conn.execute("CREATE TABLE \"O'Brien\" (id INTEGER, note VARCHAR)")
        .await
        .unwrap();

    let columns = conn.fetch_columns(None, "O'Brien").await.unwrap();

    assert_eq!(
        columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["id", "note"],
        "the table name reaches the catalog query as a bound value"
    );
}

#[test]
fn flat_files_open_with_quoted_paths_and_expected_rows() {
    let dir = TempDir::new().unwrap();
    for (extension, contents) in [
        ("csv", "id,name\n1,Ada\n2,Grace\n"),
        ("tsv", "id\tname\n1\tAda\n2\tGrace\n"),
        ("json", "[{\"id\":1,\"name\":\"Ada\"},{\"id\":2,\"name\":\"Grace\"}]"),
    ] {
        let path = dir.path().join(format!("1 O'Brien data.{extension}"));
        std::fs::write(&path, contents).unwrap();
        let conn = open_path(path.to_str().unwrap()).unwrap();
        let table = quote_ident(&derive_view_name(path.to_str().unwrap()));
        let count: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2, "{extension}");
    }
}

#[test]
fn parquet_and_json_functions_work_without_extension_downloads() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("O'Brien.parquet");
    let conn = DuckConnection::open_in_memory().unwrap();
    conn.execute_batch("SET autoinstall_known_extensions=false; SET autoload_known_extensions=false;")
        .unwrap();
    let count: i64 = conn
        .query_row("SELECT json_array_length('[1,2]')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 2);
    conn.execute_batch(&format!(
        "COPY (SELECT 1 AS id) TO '{}' (FORMAT PARQUET)",
        escape_duckdb_literal(path.to_str().unwrap())
    ))
    .unwrap();
    let reopened = open_path(path.to_str().unwrap()).unwrap();
    let count: i64 = reopened
        .query_row(
            &format!(
                "SELECT count(*) FROM {}",
                quote_ident(&derive_view_name(path.to_str().unwrap()))
            ),
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn malformed_or_missing_flat_files_report_errors() {
    let dir = TempDir::new().unwrap();
    for extension in ["json", "parquet"] {
        let path = dir.path().join(format!("broken.{extension}"));
        std::fs::write(&path, "not valid data {{{").unwrap();
        assert!(open_path(path.to_str().unwrap()).is_err());
    }
    assert!(open_path(dir.path().join("missing.csv").to_str().unwrap()).is_err());
}
