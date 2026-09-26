#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::Arc;
use std::time::Duration;

use drivers_sqlite::SqliteDriver;
use tablepro_core::{ConnectOptions, Connection, DatabaseDriver, DriverError, OperationControl, Value};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn options_for(path: &str) -> ConnectOptions {
    ConnectOptions {
        database: path.to_string(),
        ..Default::default()
    }
}

async fn connect_file(directory: &TempDir) -> Arc<dyn Connection> {
    let path = directory.path().join("cancel.db");
    let path = path.to_string_lossy().to_string();
    SqliteDriver
        .connect(options_for(&path))
        .await
        .expect("connect to the sqlite file")
        .into()
}

/// A recursive CTE counting to a large bound is pure computation, so it
/// runs long enough to be interrupted and reports `SQLITE_INTERRUPT`
/// when it is.
const LONG_QUERY: &str = "WITH RECURSIVE counter(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM counter WHERE x < 400000000) \
     SELECT count(*) FROM counter";

#[tokio::test]
async fn nullable_blob_columns_distinguish_null_empty_and_binary_values() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    connection
        .execute("CREATE TABLE blobs (id INTEGER PRIMARY KEY, payload BLOB)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO blobs VALUES (1, NULL), (2, X''), (3, X'00ff41')")
        .await
        .unwrap();
    let expected = vec![
        vec![Value::Null],
        vec![Value::Bytes(vec![])],
        vec![Value::Bytes(vec![0, 255, 65])],
    ];
    let query = connection.query("SELECT payload FROM blobs ORDER BY id").await.unwrap();
    assert_eq!(query.rows, expected);
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    let bound = connection
        .query_params_controlled(
            "SELECT payload FROM blobs WHERE id > ? ORDER BY id",
            &[Value::Int(0)],
            &control,
        )
        .await
        .unwrap();
    assert_eq!(bound.rows, expected);
}

#[tokio::test]
async fn declared_types_preserve_nulls_and_binary_affinity_mismatches() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    connection
        .execute("CREATE TABLE typed (i INTEGER, r REAL, b BOOLEAN, d DATE, t TIME, dt DATETIME, payload BLOB)")
        .await
        .unwrap();
    connection.execute("INSERT INTO typed DEFAULT VALUES").await.unwrap();
    assert_eq!(
        connection.query("SELECT * FROM typed").await.unwrap().rows,
        vec![vec![Value::Null; 7]]
    );
    connection.execute("UPDATE typed SET r = X'00ff41'").await.unwrap();
    assert_eq!(
        connection.query("SELECT r FROM typed").await.unwrap().rows,
        vec![vec![Value::Bytes(vec![0, 255, 65])]]
    );
}

#[tokio::test]
async fn undecodable_parameter_is_rejected_without_writing_null() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    connection
        .execute("CREATE TABLE values_to_keep (value INTEGER)")
        .await
        .unwrap();

    let error = connection
        .execute_params(
            "INSERT INTO values_to_keep VALUES (?)",
            &[Value::Undecodable("NUMERIC".into())],
        )
        .await
        .expect_err("an undecodable result cell cannot be written as NULL");
    assert!(matches!(error, DriverError::Unsupported(_)));

    let statements = vec![
        ("INSERT INTO values_to_keep VALUES (?)".into(), vec![Value::Int(7)]),
        (
            "INSERT INTO values_to_keep VALUES (?)".into(),
            vec![Value::Undecodable("NUMERIC".into())],
        ),
    ];
    let error = connection
        .execute_in_transaction(&statements)
        .await
        .expect_err("the transaction must reject the undecodable value");
    assert!(matches!(
        error,
        DriverError::Transaction {
            statement_index: 1,
            source,
        } if matches!(*source, DriverError::Unsupported(_))
    ));
    let rows = connection.query("SELECT count(*) FROM values_to_keep").await.unwrap();
    assert_eq!(rows.rows, vec![vec![Value::Int(0)]]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_driver_declares_server_side_cancellation() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    assert!(connection.supports_server_cancellation());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_query_is_interrupted_and_the_pool_stays_usable() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;

    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let operation_connection = connection.clone();
    let task = tokio::spawn(async move { operation_connection.query_controlled(LONG_QUERY, &control).await });

    tokio::time::sleep(Duration::from_millis(300)).await;
    token.cancel();
    let error = task
        .await
        .expect("query task")
        .expect_err("the query must be cancelled");
    assert!(matches!(error, DriverError::Cancelled), "unexpected error: {error:?}");

    let result = connection.query("SELECT 1").await.expect("the pool remains usable");
    assert_eq!(result.rows, vec![vec![Value::Int(1)]]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_timed_out_write_is_interrupted_and_reports_a_timeout() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    connection
        .execute("CREATE TABLE sink (total integer)")
        .await
        .expect("create the sink table");

    let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
    let control = OperationControl::new(CancellationToken::new(), Some(deadline));
    let sql = format!("INSERT INTO sink (total) {LONG_QUERY}");
    let operation_connection = connection.clone();
    let task = tokio::spawn(async move { operation_connection.execute_controlled(&sql, &control).await });

    let error = task.await.expect("execute task").expect_err("the write must time out");
    assert!(matches!(error, DriverError::TimedOut), "unexpected error: {error:?}");

    let rows = connection
        .query("SELECT count(*) FROM sink")
        .await
        .expect("the pool remains usable");
    assert_eq!(
        rows.rows,
        vec![vec![Value::Int(0)]],
        "the aborted insert must not commit"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_completed_query_is_unaffected_by_the_control() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    let control = OperationControl::new(CancellationToken::new(), None);

    let result = connection
        .query_controlled("SELECT 7", &control)
        .await
        .expect("an uninterrupted query returns its rows");
    assert_eq!(result.rows, vec![vec![Value::Int(7)]]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_already_cancelled_control_never_reaches_the_database() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    let token = CancellationToken::new();
    token.cancel();
    let control = OperationControl::new(token, None);

    let error = connection
        .query_controlled("SELECT 1", &control)
        .await
        .expect_err("a cancelled control must refuse before dispatch");
    assert!(matches!(error, DriverError::Cancelled), "unexpected error: {error:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_declared_structure_capability_returns_real_catalog_rows() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    connection
        .execute("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
        .await
        .expect("create the referenced table");
    connection
        .execute("CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id))")
        .await
        .expect("create the referencing table");
    connection
        .execute("CREATE UNIQUE INDEX child_parent_idx ON child(parent_id)")
        .await
        .expect("create the index");

    assert!(SqliteDriver.supports_index_metadata());
    let indexes = connection.fetch_indexes(None, "child").await.expect("fetch indexes");
    assert!(
        indexes.iter().any(|index| index.name == "child_parent_idx"),
        "a driver that declares index support must return the index it created: {indexes:?}"
    );

    assert!(SqliteDriver.supports_foreign_key_metadata());
    let foreign_keys = connection
        .fetch_foreign_keys(None, "child")
        .await
        .expect("fetch foreign keys");
    assert!(
        foreign_keys
            .iter()
            .any(|key| key.ref_table == "parent" && key.columns == vec!["parent_id".to_string()]),
        "a driver that declares foreign-key support must return the constraint it created: {foreign_keys:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_foreign_key_read_right_after_its_table_is_created_is_listed() {
    for _ in 0..5 {
        let directory = TempDir::new().expect("temp dir");
        let connection = connect_file(&directory).await;
        connection
            .execute("CREATE TABLE parent (a INTEGER, b TEXT, PRIMARY KEY (b, a))")
            .await
            .expect("create the referenced table");
        connection
            .execute("CREATE TABLE child (id INTEGER PRIMARY KEY, pa INTEGER, pb TEXT, FOREIGN KEY (pb, pa) REFERENCES parent(b, a))")
            .await
            .expect("create the referencing table");

        let foreign_keys = connection
            .fetch_foreign_keys(None, "child")
            .await
            .expect("fetch foreign keys");

        assert_eq!(foreign_keys.len(), 1, "{foreign_keys:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_foreign_key_to_the_implicit_parent_key_names_the_primary_key_columns() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    connection
        .execute("CREATE TABLE parent (a INTEGER, b TEXT, note TEXT, PRIMARY KEY (b, a))")
        .await
        .expect("create the referenced table");
    connection
        .execute(
            "CREATE TABLE child (id INTEGER PRIMARY KEY, pa INTEGER, pb TEXT, FOREIGN KEY (pb, pa) REFERENCES parent)",
        )
        .await
        .expect("create the referencing table");

    let foreign_keys = connection
        .fetch_foreign_keys(None, "child")
        .await
        .expect("fetch foreign keys");

    assert_eq!(foreign_keys.len(), 1, "{foreign_keys:?}");
    assert_eq!(foreign_keys[0].columns, vec!["pb".to_string(), "pa".to_string()]);
    assert_eq!(foreign_keys[0].ref_columns, vec!["b".to_string(), "a".to_string()]);
}

#[tokio::test]
async fn sqlite_columns_report_no_comment() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    connection
        .execute("CREATE TABLE comment_demo (id INTEGER PRIMARY KEY, label TEXT)")
        .await
        .unwrap();
    let columns = connection.fetch_columns(None, "comment_demo").await.unwrap();
    assert!(!columns.is_empty());
    assert!(columns.iter().all(|c| c.comment.is_none()));
}

async fn session_value(session: &mut Box<dyn tablepro_core::Session>, sql: &str) -> Value {
    let control = OperationControl::with_timeout(Duration::from_secs(30));
    session.query_params_controlled(sql, &[], &control).await.unwrap().rows[0][0].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_keeps_temp_tables_and_its_transaction_between_statements() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    connection.execute("CREATE TABLE ledger (id int)").await.unwrap();
    let mut session = connection.open_session().await.unwrap();
    let control = OperationControl::with_timeout(Duration::from_secs(30));

    for sql in ["CREATE TEMP TABLE scratch (n int)", "INSERT INTO scratch VALUES (7)"] {
        session.query_params_controlled(sql, &[], &control).await.unwrap();
    }
    assert_eq!(
        session_value(&mut session, "SELECT n FROM scratch").await,
        Value::Int(7)
    );
    for sql in ["BEGIN", "INSERT INTO ledger VALUES (1)", "ROLLBACK"] {
        session.query_params_controlled(sql, &[], &control).await.unwrap();
    }
    assert_eq!(
        session_value(&mut session, "SELECT count(*) FROM ledger").await,
        Value::Int(0)
    );
    session.close().await.unwrap();

    let temp = connection
        .query("SELECT count(*) FROM sqlite_temp_master")
        .await
        .unwrap();
    assert_eq!(temp.rows, vec![vec![Value::Int(0)]]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_session_statement_is_interrupted_and_the_session_stays_usable() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    let mut session = connection.open_session().await.unwrap();
    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        token.cancel();
    });

    let error = session
        .query_params_controlled(LONG_QUERY, &[], &control)
        .await
        .unwrap_err();
    canceller.await.unwrap();

    assert!(matches!(error, DriverError::Cancelled), "{error:?}");
    assert!(session.is_usable());
    assert_eq!(session_value(&mut session, "SELECT 1").await, Value::Int(1));
}

#[tokio::test]
async fn an_in_memory_database_refuses_a_session() {
    let connection = SqliteDriver.connect(options_for(":memory:")).await.unwrap();

    let error = connection
        .open_session()
        .await
        .err()
        .expect("an in-memory session is refused");

    assert!(matches!(error, DriverError::Unsupported(_)), "{error:?}");
}

#[tokio::test]
async fn fetched_defaults_keep_no_default_null_empty_and_literal_apart_and_reapply_exactly() {
    let directory = TempDir::new().expect("temp dir");
    let connection = connect_file(&directory).await;
    connection
        .execute(
            "CREATE TABLE defaults_kept (id INTEGER PRIMARY KEY, bare TEXT, explicit_null TEXT DEFAULT NULL, \
             blank TEXT DEFAULT '', word TEXT DEFAULT 'it''s', amount INTEGER DEFAULT 0)",
        )
        .await
        .unwrap();
    let columns = connection.fetch_columns(None, "defaults_kept").await.unwrap();
    let defaults: Vec<(&str, Option<&str>)> = columns
        .iter()
        .map(|c| (c.name.as_str(), c.default_value.as_deref()))
        .collect();
    assert_eq!(
        defaults,
        vec![
            ("id", None),
            ("bare", None),
            ("explicit_null", Some("NULL")),
            ("blank", Some("''")),
            ("word", Some("'it''s'")),
            ("amount", Some("0")),
        ]
    );

    for info in columns.into_iter().filter(|c| c.name != "id") {
        let mut copy = tablepro_core::sql_ddl::DraftColumn::from_info(info.clone());
        copy.original = None;
        copy.name = format!("{}_copy", info.name);
        for statement in tablepro_core::sql_ddl::build_add_column("sqlite", None, "defaults_kept", &copy).unwrap() {
            connection.execute(&statement).await.unwrap();
        }
    }
    connection
        .execute("INSERT INTO defaults_kept (id) VALUES (1)")
        .await
        .unwrap();
    let same = connection
        .query(
            "SELECT bare IS bare_copy, explicit_null IS explicit_null_copy, blank IS blank_copy, \
             word IS word_copy, amount IS amount_copy, blank_copy = '' FROM defaults_kept WHERE id = 1",
        )
        .await
        .unwrap();
    assert_eq!(same.rows[0], vec![Value::Int(1); 6]);
}

async fn notes_in(conn: &dyn Connection, table: &str) -> Vec<String> {
    let rows = conn.query(&format!("SELECT note FROM {table}")).await.unwrap();
    let mut notes: Vec<String> = rows
        .rows
        .iter()
        .map(|row| match &row[0] {
            Value::Text(text) => text.clone(),
            other => panic!("note must be text, got {other:?}"),
        })
        .collect();
    notes.sort();
    notes
}

async fn grid_edits_and_deletes_only_the_keyed_row(conn: &dyn Connection, driver_id: &str, table: &str) {
    let columns = conn.fetch_columns(None, table).await.unwrap();
    let note = columns.iter().position(|c| c.name == "note").unwrap();
    let rows = conn.fetch_rows(None, table, 0, 10).await.unwrap();
    let target = rows
        .rows
        .iter()
        .find(|row| row[note] == Value::Text("target".into()))
        .unwrap();
    let pk_values: Vec<Value> = columns
        .iter()
        .zip(target)
        .filter(|(column, _)| column.primary_key)
        .map(|(_, value)| value.clone())
        .collect();
    assert!(!pk_values.is_empty(), "{table} must report its primary key");
    let before = notes_in(conn, table).await;
    let others: Vec<String> = before.iter().filter(|n| *n != "target").cloned().collect();

    let update = tablepro_core::sql_dialect::build_keyed_update(
        driver_id,
        None,
        table,
        &columns,
        &[(note, Value::Text("edited".into()))],
        &pk_values,
    )
    .unwrap();
    assert_eq!(
        conn.execute_in_transaction(&[update]).await.unwrap(),
        vec![1],
        "{table} update"
    );
    let mut expected = others.clone();
    expected.insert(0, "edited".to_string());
    assert_eq!(
        notes_in(conn, table).await,
        expected,
        "{table} update touched only the keyed row"
    );

    let delete = tablepro_core::sql_dialect::build_keyed_delete(driver_id, None, table, &columns, &pk_values).unwrap();
    assert_eq!(
        conn.execute_in_transaction(&[delete]).await.unwrap(),
        vec![1],
        "{table} delete"
    );
    assert_eq!(
        notes_in(conn, table).await,
        others,
        "{table} delete removed only the keyed row"
    );
}

#[tokio::test]
async fn grid_row_edits_find_rows_by_uuid_composite_and_wide_bigint_keys() {
    let directory = TempDir::new().expect("temp dir");
    let conn = connect_file(&directory).await;
    for sql in [
        "CREATE TABLE keyed_uuid (id TEXT PRIMARY KEY, note TEXT)",
        "INSERT INTO keyed_uuid VALUES ('6f1c2a8e-3b4d-4e5f-8a9b-0c1d2e3f4a5b', 'target'), \
         ('6f1c2a8e-3b4d-4e5f-8a9b-0c1d2e3f4a5c', 'other')",
        "CREATE TABLE keyed_uuid_blob (id BLOB PRIMARY KEY, note TEXT)",
        "INSERT INTO keyed_uuid_blob VALUES (X'6f1c2a8e3b4d4e5f8a9b0c1d2e3f4a5b', 'target'), \
         (X'6f1c2a8e3b4d4e5f8a9b0c1d2e3f4a5c', 'other')",
        "CREATE TABLE keyed_composite (tenant INTEGER, code TEXT, note TEXT, PRIMARY KEY (tenant, code))",
        "INSERT INTO keyed_composite VALUES (1, 'a', 'target'), (1, 'b', 'other'), (2, 'a', 'other')",
        "CREATE TABLE keyed_bigint (id INTEGER PRIMARY KEY, note TEXT)",
        "INSERT INTO keyed_bigint VALUES (9007199254740993, 'target'), (9007199254740992, 'other')",
    ] {
        conn.execute(sql).await.unwrap();
    }
    for table in ["keyed_uuid", "keyed_uuid_blob", "keyed_composite", "keyed_bigint"] {
        grid_edits_and_deletes_only_the_keyed_row(conn.as_ref(), "sqlite", table).await;
    }
}
