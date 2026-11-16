#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Every controlled call that can run a statement for an unbounded time must
//! stop that statement inside DuckDB when its deadline passes, and must never
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
use drivers_duckdb::DuckdbDriver;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const STOP_AFTER: Duration = Duration::from_millis(200);
const MUST_RETURN_WITHIN: Duration = Duration::from_secs(20);

const ENDLESS_SELECT: &str = "SELECT count(*) FROM range(10000000000000) AS r(i) WHERE i % 7 = 3";
const ENDLESS_WRITE: &str = "UPDATE stop_class_rows SET note = 'stopped' \
     WHERE (SELECT count(*) FROM range(10000000000000) AS r(i) WHERE i % 7 = 3) > 0";

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
    vec![(ENDLESS_WRITE.to_string(), Vec::new())]
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
            c.execute_controlled(ENDLESS_WRITE, &control).await
        }),
        stoppable("execute_params_controlled", |c, control| async move {
            c.execute_params_controlled(ENDLESS_WRITE, &[], &control).await
        }),
        stoppable("execute_controlled_with_token", |c, control| async move {
            c.execute_controlled_with_token(ENDLESS_WRITE, &control)
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
            c.fetch_rows_controlled(None, "endless", 3_000_000_000_000, 10, &control)
                .await
        }),
    ]
}

const BOUNDED: &[(&str, &str)] = &[
    (
        "list_collations_controlled",
        "DuckDB has no collation list; the default answers at once",
    ),
    (
        "list_databases_controlled",
        "DuckDB lists no databases; the default answers at once",
    ),
    (
        "list_tables_controlled",
        "reads information_schema in process; bounded by schema size",
    ),
    (
        "list_tables_in",
        "reads information_schema in process; bounded by schema size",
    ),
    (
        "list_tables_excluding_controlled",
        "reads information_schema in process; bounded by schema size",
    ),
    (
        "list_views_controlled",
        "reads information_schema in process; bounded by schema size",
    ),
    (
        "list_views_excluding_controlled",
        "reads information_schema in process; bounded by schema size",
    ),
    (
        "lexical_settings_controlled",
        "sends no statement; the lexical settings are fixed",
    ),
    ("list_objects_controlled", "DuckDB lists no catalog objects"),
    ("list_objects_excluding_controlled", "DuckDB lists no catalog objects"),
    ("routine_definition_controlled", "DuckDB has no routine definitions"),
    (
        "fetch_columns_controlled",
        "reads information_schema in process; bounded by schema size",
    ),
    (
        "fetch_indexes_controlled",
        "reads the catalog only; bounded by schema size",
    ),
    (
        "fetch_foreign_keys_controlled",
        "reads the catalog only; bounded by schema size",
    ),
    (
        "fetch_inbound_foreign_keys_controlled",
        "reads the catalog only; bounded by schema size",
    ),
    (
        "fetch_view_editability_controlled",
        "DuckDB views are not editable; no statement is sent",
    ),
    ("fetch_enum_labels_controlled", "no statement is sent"),
    (
        "begin_controlled",
        "DuckDB offers no interactive transaction, so begin is refused before it runs",
    ),
    ("server_version_controlled", "SELECT version() runs in constant time"),
    (
        "synchronize_auto_increment_sequences_controlled",
        "DuckDB has no sequences to synchronise",
    ),
];

/// A statement that is never interrupted keeps its blocking thread for as long
/// as it runs. The default test runtime waits for that thread when it shuts
/// down, so a regression would hang the run instead of failing it.
fn run_without_waiting_for_stragglers<F: Future<Output = ()>>(test: F) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runtime.block_on(test)));
    runtime.shutdown_background();
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

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
    let declared = controlled_calls(
        include_str!("../../../../core/src/connection.rs"),
        "pub trait Connection",
        "",
    );
    let stopped: BTreeSet<String> = stop_cases().iter().map(|case| case.method.to_string()).collect();
    let bounded: BTreeSet<String> = BOUNDED.iter().map(|(name, _)| (*name).to_string()).collect();
    assert!(stopped.is_disjoint(&bounded), "a call is listed as both");
    let sorted: BTreeSet<String> = stopped.union(&bounded).cloned().collect();
    assert_eq!(sorted, declared);
}

async fn open() -> (TempDir, Arc<dyn Connection>) {
    let directory = TempDir::new().unwrap();
    let connection: Arc<dyn Connection> = DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .expect("open an in-memory duckdb database")
        .into();
    for sql in [
        "CREATE TABLE stop_class_rows (id INTEGER PRIMARY KEY, note VARCHAR)".to_string(),
        "INSERT INTO stop_class_rows VALUES (1, 'kept')".to_string(),
        "CREATE VIEW endless AS SELECT i FROM range(10000000000000) AS r(i) WHERE i % 7 = 3".to_string(),
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

#[test]
fn a_call_stopped_by_its_deadline_is_stopped_inside_duckdb_and_leaves_the_connection_usable() {
    run_without_waiting_for_stragglers(async {
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
                "{}: DuckDB aborted the statement, so the stop is confirmed, but the call returned {other:?} after {:?}",
                case.method,
                started.elapsed()
            )),
        }
            let next = tokio::time::timeout(Duration::from_secs(5), connection.query("SELECT 1")).await;
            if !matches!(next, Ok(Ok(_))) {
                failures.push(format!("{}: the next statement did not run: {next:?}", case.method));
            }
            let count = tokio::time::timeout(
                Duration::from_secs(5),
                connection.query("SELECT count(*) FROM stop_class_rows"),
            )
            .await;
            match count {
                Ok(Ok(result)) if matches!(result.rows[0][0], Value::Int(1)) => {}
                Ok(Ok(_)) => failures.push(format!("{}: a stopped call left rows behind", case.method)),
                Ok(Err(error)) => failures.push(format!("{}: the count failed: {error:?}", case.method)),
                Err(_) => failures.push(format!(
                    "{}: the statement is still running and holds the connection",
                    case.method
                )),
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
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

#[test]
fn a_caller_cancel_stops_a_checked_batch_and_nothing_is_written() {
    run_without_waiting_for_stragglers(async {
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
    });
}
