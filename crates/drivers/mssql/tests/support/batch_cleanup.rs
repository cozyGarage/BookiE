#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{connect, start_mssql};
use tablepro_core::DriverError;

#[tokio::test]
#[ignore = "requires docker"]
async fn failed_batch_rollback_is_explicit_and_retires_the_connection() {
    let (_container, options) = start_mssql().await;
    let connection = connect(options).await;
    let statements = vec![
        ("COMMIT TRANSACTION".into(), vec![]),
        ("SELECT * FROM bookie_missing_cleanup_table".into(), vec![]),
    ];
    let error = connection.execute_in_transaction(&statements).await.unwrap_err();
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
    use std::sync::Arc;
    use tablepro_core::{Connection, Value};
    let (_container, options) = start_mssql().await;
    let admin = connect(options.clone()).await;
    let connection: Arc<dyn Connection> = connect(options).await.into();
    let running = Arc::clone(&connection);
    let batch = tokio::spawn(async move {
        running
            .execute_in_transaction(&[("WAITFOR DELAY '00:00:03'; COMMIT TRANSACTION".into(), vec![])])
            .await
    });
    let mut active = false;
    for _ in 0..100 {
        let result = admin
            .query("SELECT COUNT(*) FROM sys.dm_exec_requests WHERE session_id <> @@SPID AND wait_type = 'WAITFOR'")
            .await
            .unwrap();
        if result.rows == vec![vec![Value::Int(1)]] {
            active = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        active,
        "the batch must hold the shared client before queueing the query"
    );
    let query = connection.query("SELECT 1");
    tokio::pin!(query);
    assert!(futures::poll!(&mut query).is_pending());
    assert!(matches!(
        batch.await.unwrap(),
        Err(DriverError::TransactionRollbackFailed { .. })
    ));
    assert!(matches!(query.await, Err(DriverError::Disconnected)));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn confirmed_rollback_keeps_the_parameter_failure_and_client_usable() {
    use tablepro_core::Value;
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
