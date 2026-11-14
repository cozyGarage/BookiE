#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{connect, start_mssql};
use bookie_core::DriverError;

use bookie_core::{Connection, Value};
use std::sync::Arc;

async fn wait_for_the_batch_to_hold_the_client(admin: &dyn Connection) -> i64 {
    for _ in 0..100 {
        let result = admin
            .query("SELECT session_id FROM sys.dm_exec_requests WHERE session_id <> @@SPID AND wait_type = 'WAITFOR'")
            .await
            .unwrap();
        if let Some(Value::Int(session)) = result.rows.first().and_then(|row| row.first()) {
            return *session;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the batch must hold the shared client before the session is killed");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn failed_batch_rollback_is_explicit_and_retires_the_connection() {
    let (_container, options) = start_mssql().await;
    let admin = connect(options.clone()).await;
    let connection: Arc<dyn Connection> = connect(options).await.into();
    let running = Arc::clone(&connection);
    let batch = tokio::spawn(async move {
        running
            .execute_in_transaction(&[("WAITFOR DELAY '00:00:30'".into(), vec![])])
            .await
    });
    let session = wait_for_the_batch_to_hold_the_client(admin.as_ref()).await;
    admin.execute(&format!("KILL {session}")).await.unwrap();

    let error = batch.await.unwrap().unwrap_err();
    assert!(
        matches!(error, DriverError::TransactionRollbackFailed { statement_index: 0, .. }),
        "{error:?}"
    );
    assert!(matches!(
        connection.query("SELECT 1").await,
        Err(DriverError::Disconnected)
    ));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_queued_query_cannot_use_a_client_retired_by_failed_cleanup() {
    let (_container, options) = start_mssql().await;
    let admin = connect(options.clone()).await;
    let connection: Arc<dyn Connection> = connect(options).await.into();
    let running = Arc::clone(&connection);
    let batch = tokio::spawn(async move {
        running
            .execute_in_transaction(&[("WAITFOR DELAY '00:00:30'".into(), vec![])])
            .await
    });
    let session = wait_for_the_batch_to_hold_the_client(admin.as_ref()).await;
    let query = connection.query("SELECT 1");
    tokio::pin!(query);
    assert!(futures::poll!(&mut query).is_pending());
    admin.execute(&format!("KILL {session}")).await.unwrap();
    assert!(matches!(
        batch.await.unwrap(),
        Err(DriverError::TransactionRollbackFailed { .. })
    ));
    assert!(matches!(query.await, Err(DriverError::Disconnected)));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn confirmed_rollback_keeps_the_parameter_failure_and_client_usable() {
    let (_container, options) = start_mssql().await;
    let connection = connect(options).await;
    let result = connection
        .execute_in_transaction(&[(
            "SELECT @P1".into(),
            vec![Value::Undecodable("unsupported fixture value".into())],
        )])
        .await;
    assert!(
        matches!(result, Err(DriverError::Transaction { statement_index: 0, source })
        if matches!(source.as_ref(), DriverError::Unsupported(_)))
    );
    assert_eq!(
        connection.query("SELECT 1").await.unwrap().rows,
        vec![vec![Value::Int(1)]]
    );
}
