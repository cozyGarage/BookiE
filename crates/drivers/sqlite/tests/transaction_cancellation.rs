#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A Stop pressed while a batch transaction is running must end it as a
//! confirmed cancellation: SQLite aborts the statement and rolls the
//! transaction back, so the caller learns nothing was written and the
//! connection stays usable.

use std::time::Duration;

use bookie_core::{ConnectOptions, Connection, DatabaseDriver, DriverError, OperationControl, Value};
use drivers_sqlite::SqliteDriver;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const ENDLESS_INSERT: &str = "WITH RECURSIVE counter(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM counter WHERE x < 400000000) \
     INSERT INTO t (id) SELECT x FROM counter";

async fn open() -> (TempDir, Box<dyn Connection>) {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("cancel.db");
    let _ = std::fs::File::create(&path);
    let connection = SqliteDriver
        .connect(ConnectOptions {
            database: path.to_string_lossy().to_string(),
            ..Default::default()
        })
        .await
        .expect("connect to the sqlite file");
    connection
        .execute("CREATE TABLE t (id INTEGER PRIMARY KEY)")
        .await
        .expect("create the table");
    (directory, connection)
}

fn control(token: CancellationToken) -> OperationControl {
    OperationControl::new(token, Some(tokio::time::Instant::now() + Duration::from_secs(60)))
}

async fn row_count(connection: &dyn Connection) -> i64 {
    let result = connection.query("SELECT count(*) FROM t").await.unwrap();
    match &result.rows[0][0] {
        Value::Int(count) => *count,
        other => panic!("count was {other:?}"),
    }
}

#[tokio::test]
async fn a_stop_during_a_batch_transaction_is_a_confirmed_cancellation_with_nothing_written() {
    let (_directory, connection) = open().await;
    let token = CancellationToken::new();
    let stopper = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        stopper.cancel();
    });

    let statements = vec![(ENDLESS_INSERT.to_string(), Vec::new())];
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        connection.execute_in_transaction_controlled(&statements, &control(token)),
    )
    .await
    .expect("the stop must end the transaction instead of letting it run on");

    assert!(
        matches!(result, Err(DriverError::Cancelled)),
        "a stop SQLite confirmed must be Cancelled, not an unknown outcome: {result:?}"
    );
    assert_eq!(
        row_count(connection.as_ref()).await,
        0,
        "the batch must have rolled back"
    );

    connection
        .execute_in_transaction(&[("INSERT INTO t (id) VALUES (?)".into(), vec![Value::Int(1)])])
        .await
        .expect("the connection must accept the next write");
    assert_eq!(row_count(connection.as_ref()).await, 1);
}

#[tokio::test]
async fn an_uninterrupted_controlled_transaction_commits_every_statement() {
    let (_directory, connection) = open().await;
    let statements: Vec<(String, Vec<Value>)> = (1..=3)
        .map(|id| ("INSERT INTO t (id) VALUES (?)".to_string(), vec![Value::Int(id)]))
        .collect();

    let affected = connection
        .execute_in_transaction_controlled(&statements, &control(CancellationToken::new()))
        .await
        .unwrap();

    assert_eq!(affected, vec![1, 1, 1]);
    assert_eq!(row_count(connection.as_ref()).await, 3);
}

#[tokio::test]
async fn a_statement_error_in_a_controlled_transaction_rolls_everything_back() {
    let (_directory, connection) = open().await;
    let statements = vec![
        ("INSERT INTO t (id) VALUES (1)".to_string(), Vec::new()),
        ("INSERT INTO t (id) VALUES (1)".to_string(), Vec::new()),
    ];

    let result = connection
        .execute_in_transaction_controlled(&statements, &control(CancellationToken::new()))
        .await;

    assert!(
        matches!(result, Err(DriverError::Transaction { statement_index: 1, .. })),
        "{result:?}"
    );
    assert_eq!(row_count(connection.as_ref()).await, 0);
}

#[tokio::test]
async fn a_stop_between_short_statements_is_also_a_confirmed_cancellation() {
    let (_directory, connection) = open().await;
    let token = CancellationToken::new();
    let stopper = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        stopper.cancel();
    });

    let statements: Vec<(String, Vec<Value>)> = (1..=400_000)
        .map(|id| ("INSERT INTO t (id) VALUES (?)".to_string(), vec![Value::Int(id)]))
        .collect();
    let result = tokio::time::timeout(
        Duration::from_secs(60),
        connection.execute_in_transaction_controlled(&statements, &control(token)),
    )
    .await
    .expect("the stop must end the transaction instead of letting it run on");

    assert!(
        matches!(result, Err(DriverError::Cancelled)),
        "a stop between statements must be Cancelled, not an unknown outcome: {result:?}"
    );
    assert_eq!(
        row_count(connection.as_ref()).await,
        0,
        "the batch must have rolled back"
    );
}
