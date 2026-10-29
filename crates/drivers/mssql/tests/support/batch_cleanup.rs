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
