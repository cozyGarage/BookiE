#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use super::start_mssql;
use bookie_core::{Connection, DatabaseDriver, DriverError, OperationControl};
use drivers_mssql::MssqlDriver;

#[tokio::test]
#[ignore = "requires docker"]
async fn an_interrupted_transaction_batch_retires_the_connection_instead_of_leaving_it_open() {
    let (_c, opts) = start_mssql().await;
    let connection: Arc<dyn Connection> = MssqlDriver.connect(opts).await.expect("connect").into();
    connection
        .execute("CREATE TABLE tx_wait (id int NOT NULL PRIMARY KEY)")
        .await
        .unwrap();

    let token = tokio_util::sync::CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let running = connection.clone();
    let task = tokio::spawn(async move {
        let statements = vec![
            ("INSERT INTO tx_wait VALUES (1)".to_string(), Vec::new()),
            ("WAITFOR DELAY '00:00:30'".to_string(), Vec::new()),
        ];
        running.execute_in_transaction_controlled(&statements, &control).await
    });
    tokio::time::sleep(Duration::from_millis(1500)).await;
    token.cancel();

    let error = task
        .await
        .expect("transaction task")
        .expect_err("an interrupted batch must not succeed");
    assert!(
        matches!(error, DriverError::OperationOutcomeUnknown { .. }),
        "the batch may still be running: {error:?}"
    );

    let reused = tokio::time::timeout(Duration::from_secs(5), connection.query("SELECT @@TRANCOUNT"))
        .await
        .expect("a retired connection must answer instead of hanging");
    assert!(
        matches!(reused, Err(DriverError::Disconnected)),
        "the connection must not stay usable inside the open transaction: {reused:?}"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_batch_the_server_already_rolled_back_is_a_clean_transaction_failure() {
    let (_c, opts) = start_mssql().await;
    let connection = MssqlDriver.connect(opts).await.expect("connect");
    connection
        .execute("CREATE TABLE tx_trigger (id int NOT NULL PRIMARY KEY)")
        .await
        .unwrap();
    connection
        .execute("EXEC('CREATE TRIGGER tx_trigger_rollback ON tx_trigger AFTER INSERT AS ROLLBACK TRANSACTION')")
        .await
        .unwrap();

    let statements = vec![("INSERT INTO tx_trigger VALUES (1)".to_string(), Vec::new())];
    for error in [
        connection.execute_in_transaction(&statements).await.unwrap_err(),
        connection
            .execute_in_transaction_checked(&statements, &[0])
            .await
            .unwrap_err(),
    ] {
        assert!(
            matches!(error, DriverError::Transaction { statement_index: 0, .. }),
            "a rollback the server already did is not a failed rollback: {error:?}"
        );
    }

    let rows = connection.query("SELECT COUNT(*) FROM tx_trigger").await.unwrap();
    assert_eq!(
        rows.rows.len(),
        1,
        "the connection stays usable after the server rolled back"
    );
}
