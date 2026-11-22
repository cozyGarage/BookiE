#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Every controlled call that can run a statement for an unbounded time must
//! stop that statement inside SQLite when its deadline passes, and must never
//! return a connection or an open transaction that holds an uncertain
//! statement. The calls that cannot run unboundedly are listed with the
//! reason, so a new controlled call has to be sorted into one list or the
//! other.

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bookie_core::{ConnectOptions, Connection, DatabaseDriver, DriverError, OperationControl, Value};
use drivers_sqlite::SqliteDriver;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const STOP_AFTER: Duration = Duration::from_millis(200);
const MUST_RETURN_WITHIN: Duration = Duration::from_secs(20);

const ENDLESS_COUNT: &str = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 4000000000) ";
const ENDLESS_SELECT: &str = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 4000000000) \
     SELECT count(*) FROM c";
const ENDLESS_INSERT: &str = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 4000000000) \
     INSERT INTO stop_class_rows (id) SELECT x + 1000 FROM c";

type CallFuture = Pin<Box<dyn Future<Output = Result<(), DriverError>> + Send>>;
type Call = Box<dyn Fn(Arc<dyn Connection>, OperationControl) -> CallFuture + Send + Sync>;

struct StopCase {
    method: &'static str,
    call: Call,
}

fn stoppable<F, Fut, T>(method: &'static str, call: F) -> StopCase
where
    F: Fn(Arc<dyn Connection>, OperationControl) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DriverError>> + Send + 'static,
    T: Send,
{
    StopCase {
        method,
        call: Box::new(move |connection, control| {
            let call = call(connection, control);
            Box::pin(async move { call.await.map(|_| ()) })
        }),
    }
}

fn endless_batch() -> Vec<(String, Vec<Value>)> {
    vec![(ENDLESS_INSERT.to_string(), Vec::new())]
}

fn stop_cases() -> Vec<StopCase> {
    vec![
        stoppable("query_controlled", |c, control| async move {
            c.query_controlled(ENDLESS_SELECT, &control).await
        }),
        stoppable("query_params_controlled", |c, control| async move {
            c.query_params_controlled(ENDLESS_SELECT, &[], &control).await
        }),
        stoppable("query_result_sets_controlled", |c, control| async move {
            c.query_result_sets_controlled(ENDLESS_SELECT, &[], &control).await
        }),
        stoppable("execute_controlled", |c, control| async move {
            c.execute_controlled(ENDLESS_INSERT, &control).await
        }),
        stoppable("execute_params_controlled", |c, control| async move {
            c.execute_params_controlled(ENDLESS_INSERT, &[], &control).await
        }),
        stoppable("execute_controlled_with_token", |c, control| async move {
            c.execute_controlled_with_token(ENDLESS_INSERT, &control)
                .await
                .map_err(|(error, _)| error)
        }),
        stoppable("execute_in_transaction_controlled", |c, control| async move {
            c.execute_in_transaction_controlled(&endless_batch(), &control).await
        }),
        stoppable("execute_in_transaction_checked_controlled", |c, control| async move {
            c.execute_in_transaction_checked_controlled(&endless_batch(), &[0], &control)
                .await
        }),
        stoppable("fetch_rows_controlled", |c, control| async move {
            c.fetch_rows_controlled(None, "endless", 3_000_000_000, 10, &control)
                .await
        }),
        stoppable("Transaction::query_controlled", |c, control| async move {
            c.begin().await?.query_controlled(ENDLESS_SELECT, &control).await
        }),
        stoppable("Transaction::execute_controlled", |c, control| async move {
            c.begin().await?.execute_controlled(ENDLESS_INSERT, &control).await
        }),
        stoppable("Transaction::query_params_controlled", |c, control| async move {
            c.begin()
                .await?
                .query_params_controlled(ENDLESS_SELECT, &[], &control)
                .await
        }),
        stoppable("Transaction::execute_params_controlled", |c, control| async move {
            c.begin()
                .await?
                .execute_params_controlled(ENDLESS_INSERT, &[], &control)
                .await
        }),
    ]
}

const BOUNDED: &[(&str, &str)] = &[
    (
        "list_collations_controlled",
        "reads the in-process collation list; bounded by sqlx's default 5 s busy timeout",
    ),
    (
        "list_databases_controlled",
        "reads the in-process database list; bounded by sqlx's default 5 s busy timeout",
    ),
    (
        "list_tables_controlled",
        "catalog read in process; bounded by schema size and sqlx's default 5 s busy timeout",
    ),
    (
        "list_tables_in",
        "catalog read in process; bounded by schema size and sqlx's default 5 s busy timeout",
    ),
    (
        "list_tables_excluding_controlled",
        "catalog read in process; bounded by schema size and sqlx's default 5 s busy timeout",
    ),
    (
        "list_views_controlled",
        "catalog read in process; bounded by schema size and sqlx's default 5 s busy timeout",
    ),
    (
        "list_views_excluding_controlled",
        "catalog read in process; bounded by schema size and sqlx's default 5 s busy timeout",
    ),
    (
        "lexical_settings_controlled",
        "sends no statement; the lexical settings are fixed",
    ),
    ("list_objects_controlled", "SQLite lists no catalog objects"),
    ("list_objects_excluding_controlled", "SQLite lists no catalog objects"),
    ("routine_definition_controlled", "SQLite has no routines"),
    (
        "fetch_columns_controlled",
        "PRAGMA table_info prepares the table and runs nothing over its rows",
    ),
    (
        "fetch_schema_table_metadata_controlled",
        "catalog reads are bounded by the requested tables and their schema metadata",
    ),
    (
        "fetch_indexes_controlled",
        "PRAGMA index_list and index_info read the catalog only",
    ),
    (
        "fetch_foreign_keys_controlled",
        "PRAGMA foreign_key_list reads the catalog only",
    ),
    (
        "fetch_inbound_foreign_keys_controlled",
        "reads the foreign keys of each table; bounded by schema size",
    ),
    (
        "fetch_view_editability_controlled",
        "SQLite views are not editable; no statement is sent",
    ),
    ("fetch_enum_labels_controlled", "SQLite has no enum types"),
    (
        "begin_controlled",
        "BEGIN is deferred and takes no lock until the first statement",
    ),
    (
        "server_version_controlled",
        "SELECT sqlite_version() runs in constant time",
    ),
    (
        "synchronize_auto_increment_sequences_controlled",
        "SQLite has no sequences to synchronise",
    ),
    (
        "Transaction::rollback_controlled",
        "ROLLBACK is bounded by the transaction's own size; a deadline abandons the wait and the rollback finishes on its own task",
    ),
];

fn controlled_calls(source: &str, contract: &str, prefix: &str) -> BTreeSet<String> {
    source
        .split(contract)
        .nth(1)
        .expect("the contract trait")
        .split("#[cfg(test)]")
        .next()
        .unwrap_or_default()
        .split("async fn ")
        .skip(1)
        .filter(|rest| {
            let signature = rest.split(['{', ';']).next().unwrap_or_default();
            signature.contains("OperationControl")
        })
        .filter_map(|rest| rest.split('(').next())
        .map(|name| format!("{prefix}{}", name.trim()))
        .collect()
}

#[test]
fn every_controlled_call_of_the_contract_is_stoppable_or_bounded_with_a_reason() {
    let mut declared = controlled_calls(
        include_str!("../../../../core/src/connection.rs"),
        "pub trait Connection",
        "",
    );
    declared.extend(controlled_calls(
        include_str!("../../../../core/src/transaction.rs"),
        "pub trait Transaction",
        "Transaction::",
    ));
    let stopped: BTreeSet<String> = stop_cases().iter().map(|case| case.method.to_string()).collect();
    let bounded: BTreeSet<String> = BOUNDED.iter().map(|(name, _)| (*name).to_string()).collect();
    assert!(stopped.is_disjoint(&bounded), "a call is listed as both");
    let sorted: BTreeSet<String> = stopped.union(&bounded).cloned().collect();
    assert_eq!(sorted, declared);
}

async fn open() -> (TempDir, Arc<dyn Connection>) {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("stop.db");
    let _ = std::fs::File::create(&path);
    let connection: Arc<dyn Connection> = SqliteDriver
        .connect(ConnectOptions {
            database: path.to_string_lossy().to_string(),
            ..Default::default()
        })
        .await
        .expect("connect to the sqlite file")
        .into();
    for sql in [
        "CREATE TABLE stop_class_rows (id INTEGER PRIMARY KEY, note TEXT)".to_string(),
        "INSERT INTO stop_class_rows (id, note) VALUES (1, 'kept')".to_string(),
        format!("CREATE VIEW endless AS {ENDLESS_COUNT} SELECT x FROM c"),
    ] {
        connection.execute(&sql).await.unwrap();
    }
    (directory, connection)
}

async fn row_count(connection: &dyn Connection) -> i64 {
    let result = connection.query("SELECT count(*) FROM stop_class_rows").await.unwrap();
    match &result.rows[0][0] {
        Value::Int(count) => *count,
        other => panic!("count was {other:?}"),
    }
}

#[tokio::test]
async fn a_call_stopped_by_its_deadline_is_stopped_inside_sqlite_and_leaves_the_connection_usable() {
    let mut failures = Vec::new();
    for case in stop_cases() {
        let (_directory, connection) = open().await;
        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(
            MUST_RETURN_WITHIN,
            (case.call)(connection.clone(), OperationControl::with_timeout(STOP_AFTER)),
        )
        .await;
        match outcome {
            Err(_) => failures.push(format!("{}: still running after {MUST_RETURN_WITHIN:?}", case.method)),
            Ok(Err(DriverError::TimedOut)) => {}
            Ok(other) => failures.push(format!(
                "{}: SQLite aborted the statement, so the stop is confirmed, but the call returned {other:?} after {:?}",
                case.method,
                started.elapsed()
            )),
        }
        let next = tokio::time::timeout(Duration::from_secs(5), connection.query("SELECT 1")).await;
        if !matches!(next, Ok(Ok(_))) {
            failures.push(format!("{}: the next statement did not run: {next:?}", case.method));
        }
        match connection.query("SELECT count(*) FROM stop_class_rows").await {
            Ok(result) if matches!(result.rows[0][0], Value::Int(1)) => {}
            Ok(_) => failures.push(format!("{}: a stopped call left rows behind", case.method)),
            Err(error) => failures.push(format!(
                "{}: the statement is still running and holds the database: {error:?}",
                case.method
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn a_checked_batch_still_commits_and_still_refuses_a_row_that_changed() {
    let (_directory, connection) = open().await;
    let control = OperationControl::with_timeout(Duration::from_secs(30));

    let update = vec![(
        "UPDATE stop_class_rows SET note = ? WHERE id = 1".to_string(),
        vec![Value::Text("edited".into())],
    )];
    let affected = connection
        .execute_in_transaction_checked_controlled(&update, &[0], &control)
        .await
        .unwrap();
    assert_eq!(affected, vec![1]);

    let missing = vec![(
        "UPDATE stop_class_rows SET note = ? WHERE id = 99".to_string(),
        vec![Value::Text("lost".into())],
    )];
    let error = connection
        .execute_in_transaction_checked_controlled(&missing, &[0], &control)
        .await
        .expect_err("a keyed write that changes no row is refused");
    assert!(
        matches!(&error, DriverError::Transaction { source, .. } if matches!(source.as_ref(), DriverError::ConcurrentModification)),
        "{error:?}"
    );
    let note = connection
        .query("SELECT note FROM stop_class_rows WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(note.rows, vec![vec![Value::Text("edited".into())]]);
}

#[tokio::test]
async fn a_caller_cancel_stops_a_checked_batch_and_nothing_is_written() {
    let (_directory, connection) = open().await;
    let token = CancellationToken::new();
    let stopper = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(STOP_AFTER).await;
        stopper.cancel();
    });
    let result = tokio::time::timeout(
        MUST_RETURN_WITHIN,
        connection.execute_in_transaction_checked_controlled(
            &endless_batch(),
            &[0],
            &OperationControl::new(token, None),
        ),
    )
    .await
    .expect("the cancel must end the batch");
    assert!(matches!(result, Err(DriverError::Cancelled)), "{result:?}");
    assert_eq!(row_count(connection.as_ref()).await, 1);
}

#[tokio::test]
async fn a_stopped_write_in_a_sqlite_transaction_ends_the_transaction() {
    let (_directory, connection) = open().await;
    let mut transaction = connection.begin().await.unwrap();
    let error = transaction
        .execute_controlled(ENDLESS_INSERT, &OperationControl::with_timeout(STOP_AFTER))
        .await
        .expect_err("the endless write is stopped");
    assert!(matches!(error, DriverError::TimedOut), "{error:?}");

    let later = transaction.execute("INSERT INTO stop_class_rows (id) VALUES (2)").await;
    assert!(
        later.is_err(),
        "SQLite rolled the transaction back, so a later statement must not run on its own"
    );
    assert_eq!(row_count(connection.as_ref()).await, 1);
    assert!(transaction.commit().await.is_err());
    assert_eq!(row_count(connection.as_ref()).await, 1);
}

#[tokio::test]
async fn a_stopped_read_in_a_sqlite_transaction_leaves_the_transaction_usable() {
    let (_directory, connection) = open().await;
    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute("INSERT INTO stop_class_rows (id) VALUES (2)")
        .await
        .unwrap();
    let error = transaction
        .query_controlled(ENDLESS_SELECT, &OperationControl::with_timeout(STOP_AFTER))
        .await
        .expect_err("the endless read is stopped");
    assert!(matches!(error, DriverError::TimedOut), "{error:?}");

    transaction
        .execute("INSERT INTO stop_class_rows (id) VALUES (3)")
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(row_count(connection.as_ref()).await, 3);
}

#[tokio::test]
async fn a_sqlite_transaction_refuses_bound_parameters_without_running_anything() {
    let (_directory, connection) = open().await;
    let mut transaction = connection.begin().await.unwrap();
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    let params = [Value::Int(1)];

    let query = transaction
        .query_params_controlled("SELECT ?", &params, &control)
        .await
        .expect_err("a transaction refuses bound parameters");
    let execute = transaction
        .execute_params_controlled("INSERT INTO stop_class_rows (id) VALUES (?)", &params, &control)
        .await
        .expect_err("a transaction refuses bound parameters");
    assert!(matches!(query, DriverError::Unsupported(_)), "{query:?}");
    assert!(matches!(execute, DriverError::Unsupported(_)), "{execute:?}");
    transaction.commit().await.unwrap();
    assert_eq!(row_count(connection.as_ref()).await, 1);
}

async fn second_connection(directory: &TempDir) -> Arc<dyn Connection> {
    SqliteDriver
        .connect(ConnectOptions {
            database: directory.path().join("stop.db").to_string_lossy().to_string(),
            ..Default::default()
        })
        .await
        .expect("a second connection to the same file")
        .into()
}

async fn the_pool_stays_clean_for_later_transactions(connection: &dyn Connection, other: &dyn Connection) {
    assert_eq!(row_count(other).await, 1);
    for round in 0..8_i64 {
        let id = 100 + round;
        let mut transaction = connection.begin().await.unwrap();
        transaction
            .execute(&format!("INSERT INTO stop_class_rows (id) VALUES ({id})"))
            .await
            .unwrap();
        transaction.rollback().await.unwrap();
        assert_eq!(
            row_count(other).await,
            1,
            "a rolled back transaction left rows behind in round {round}"
        );
    }
    for round in 0..8_i64 {
        let id = 200 + round;
        connection
            .execute(&format!("INSERT INTO stop_class_rows (id) VALUES ({id})"))
            .await
            .unwrap();
        assert_eq!(
            row_count(other).await,
            2 + round,
            "an acknowledged write is not visible to another connection in round {round}"
        );
    }
    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute("INSERT INTO stop_class_rows (id) VALUES (300)")
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(row_count(other).await, 10);
}

#[tokio::test]
async fn a_stopped_write_in_a_sqlite_transaction_leaves_the_pool_clean_for_later_transactions() {
    let (directory, connection) = open().await;
    let other = second_connection(&directory).await;
    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute_controlled(ENDLESS_INSERT, &OperationControl::with_timeout(STOP_AFTER))
        .await
        .expect_err("the endless write is stopped");
    drop(transaction);

    the_pool_stays_clean_for_later_transactions(connection.as_ref(), other.as_ref()).await;
}

#[tokio::test]
async fn a_stopped_batch_leaves_the_pool_clean_for_later_transactions() {
    let (directory, connection) = open().await;
    let other = second_connection(&directory).await;
    let result = connection
        .execute_in_transaction_checked_controlled(&endless_batch(), &[0], &OperationControl::with_timeout(STOP_AFTER))
        .await;
    assert!(matches!(result, Err(DriverError::TimedOut)), "{result:?}");

    the_pool_stays_clean_for_later_transactions(connection.as_ref(), other.as_ref()).await;
}

#[tokio::test]
async fn a_stopped_unchecked_batch_leaves_the_pool_clean_for_later_transactions() {
    let (directory, connection) = open().await;
    let other = second_connection(&directory).await;
    let result = connection
        .execute_in_transaction_controlled(&endless_batch(), &OperationControl::with_timeout(STOP_AFTER))
        .await;
    assert!(matches!(result, Err(DriverError::TimedOut)), "{result:?}");

    the_pool_stays_clean_for_later_transactions(connection.as_ref(), other.as_ref()).await;
}

async fn wait_until_the_database_accepts_a_write(other: &dyn Connection) {
    for _ in 0..120 {
        if other
            .execute("INSERT INTO stop_class_rows (id) VALUES (999)")
            .await
            .is_ok()
        {
            other
                .execute("DELETE FROM stop_class_rows WHERE id = 999")
                .await
                .unwrap();
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("the database still refuses writes a minute later");
}

#[tokio::test]
async fn a_write_that_cannot_be_interrupted_in_time_leaves_the_pool_clean_once_it_ends() {
    let (directory, connection) = open().await;
    let other = second_connection(&directory).await;
    let slow = vec!["length(randomblob(300000000))"; 28].join(" + ");
    let write = format!("UPDATE stop_class_rows SET note = 'slow' WHERE {slow} > 0");

    let mut transaction = connection.begin().await.unwrap();
    let error = transaction
        .execute_controlled(&write, &OperationControl::with_timeout(STOP_AFTER))
        .await
        .expect_err("the slow write is stopped or left unknown");
    assert!(
        matches!(
            error,
            DriverError::TimedOut | DriverError::OperationOutcomeUnknown { .. }
        ),
        "{error:?}"
    );
    drop(transaction);

    wait_until_the_database_accepts_a_write(other.as_ref()).await;
    the_pool_stays_clean_for_later_transactions(connection.as_ref(), other.as_ref()).await;
}

#[tokio::test]
async fn a_transaction_ended_by_its_own_text_leaves_the_pool_clean() {
    for ending in ["COMMIT", "ROLLBACK"] {
        let (directory, connection) = open().await;
        let other = second_connection(&directory).await;
        let mut transaction = connection.begin().await.unwrap();
        transaction.execute(ending).await.unwrap();
        let outcome = if ending == "COMMIT" {
            transaction.rollback().await
        } else {
            transaction.commit().await
        };
        assert_eq!(outcome.is_ok(), ending == "COMMIT", "{ending}: {outcome:?}");

        the_pool_stays_clean_for_later_transactions(connection.as_ref(), other.as_ref()).await;
    }
}

#[tokio::test]
async fn a_transaction_dropped_after_its_own_commit_text_leaves_the_pool_clean() {
    let (directory, connection) = open().await;
    let other = second_connection(&directory).await;
    let mut transaction = connection.begin().await.unwrap();
    transaction.execute("COMMIT").await.unwrap();
    drop(transaction);

    the_pool_stays_clean_for_later_transactions(connection.as_ref(), other.as_ref()).await;
}

#[tokio::test]
async fn a_rollback_refused_at_its_deadline_leaves_the_pool_clean_after_the_transactions_own_commit_text() {
    let (directory, connection) = open().await;
    let other = second_connection(&directory).await;
    let mut transaction = connection.begin().await.unwrap();
    transaction.execute("COMMIT").await.unwrap();
    let result = transaction
        .rollback_controlled(&OperationControl::with_timeout(Duration::ZERO))
        .await;
    assert!(result.is_err(), "an expired control refuses the rollback");

    the_pool_stays_clean_for_later_transactions(connection.as_ref(), other.as_ref()).await;
}

#[tokio::test]
async fn a_controlled_rollback_after_the_transactions_own_commit_text_succeeds_and_leaves_the_pool_clean() {
    let (directory, connection) = open().await;
    let other = second_connection(&directory).await;
    let mut transaction = connection.begin().await.unwrap();
    transaction.execute("COMMIT").await.unwrap();
    transaction
        .rollback_controlled(&OperationControl::with_timeout(Duration::from_secs(30)))
        .await
        .unwrap();

    the_pool_stays_clean_for_later_transactions(connection.as_ref(), other.as_ref()).await;
}
