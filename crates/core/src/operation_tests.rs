use std::future::pending;
use std::time::Duration;

use super::*;

#[tokio::test]
async fn a_finished_call_leaves_the_callers_token_usable_for_the_next_call() {
    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), Some(Instant::now() + Duration::from_secs(30)));
    let failure = || DriverError::query("statement failed");

    assert_eq!(
        run_controlled(async { Ok::<_, DriverError>(1) }, &control)
            .await
            .unwrap(),
        1
    );
    assert!(
        run_controlled(async { Err::<(), _>(failure()) }, &control)
            .await
            .is_err()
    );
    assert_eq!(run_controlled_setup(async { 2 }, &control).await.unwrap(), 2);
    let never_cancel = || async { Ok::<_, DriverError>(()) };
    assert_eq!(
        run_server_cancellable(async { Ok(3) }, never_cancel(), |_| false, &control)
            .await
            .unwrap(),
        3
    );
    assert!(
        run_server_cancellable(async { Err::<(), _>(failure()) }, never_cancel(), |_| false, &control)
            .await
            .is_err()
    );

    assert!(!token.is_cancelled(), "a finished call cancelled its caller's token");
    assert!(check_pre_dispatch(&control).is_ok());
    assert_eq!(
        run_controlled(async { Ok::<_, DriverError>(4) }, &control)
            .await
            .unwrap(),
        4
    );
}

#[tokio::test]
async fn a_completed_operation_never_dispatches_server_cancellation() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let dispatched = Arc::new(AtomicBool::new(false));
    let dispatched_by_cancel = Arc::clone(&dispatched);
    let cancellation = async move {
        dispatched_by_cancel.store(true, Ordering::SeqCst);
        Ok::<_, DriverError>(())
    };

    let result = run_server_cancellable(async { Ok(5) }, cancellation, |_| false, &control).await;

    assert_eq!(result.unwrap(), 5);
    assert!(
        !dispatched.load(Ordering::SeqCst),
        "a finished call asked the server to stop"
    );
    assert!(!token.is_cancelled());
}

#[tokio::test]
async fn a_cancel_after_dispatch_is_an_unknown_outcome_not_a_clean_cancel() {
    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let canceller = token.clone();
    let operation = async move {
        canceller.cancel();
        std::future::pending::<Result<(), DriverError>>().await
    };

    let error = run_controlled(operation, &control).await.unwrap_err();

    assert!(
        matches!(&error, DriverError::OperationOutcomeUnknown { source } if matches!(**source, DriverError::Cancelled)),
        "{error:?}"
    );
}

#[tokio::test]
async fn returns_completed_operation_result() {
    let control = OperationControl::new(CancellationToken::new(), None);

    let result = run_controlled(async { Ok::<_, DriverError>(42) }, &control).await;

    assert_eq!(result.expect("operation should complete"), 42);
}

#[tokio::test]
async fn reports_pre_dispatch_cancellation() {
    let token = CancellationToken::new();
    token.cancel();
    let control = OperationControl::new(token, None);

    let error = run_controlled(pending::<Result<(), DriverError>>(), &control)
        .await
        .expect_err("operation should be interrupted");

    assert!(matches!(error, DriverError::Cancelled));
}

#[tokio::test]
async fn reports_pre_dispatch_timeout() {
    let deadline = Instant::now() - Duration::from_millis(1);
    let control = OperationControl::new(CancellationToken::new(), Some(deadline));

    let error = run_controlled(pending::<Result<(), DriverError>>(), &control)
        .await
        .expect_err("operation should be interrupted");

    assert!(matches!(error, DriverError::TimedOut));
}

#[tokio::test]
async fn with_timeout_sets_a_deadline_in_the_future() {
    let before = Instant::now();
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    let deadline = control.deadline().expect("timeout must produce a deadline");
    assert!(deadline > before);
    assert!(deadline <= before + Duration::from_secs(6));
}

#[tokio::test]
async fn cancellation_wins_when_both_controls_are_ready() {
    let token = CancellationToken::new();
    token.cancel();
    let control = OperationControl::new(token, Some(Instant::now()));

    let error = run_controlled(pending::<Result<(), DriverError>>(), &control)
        .await
        .expect_err("operation should be interrupted");

    assert!(matches!(error, DriverError::Cancelled));
}

#[tokio::test]
async fn completed_operation_waits_for_an_in_flight_server_cancel_dispatch() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::oneshot;

    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let (operation_started_tx, operation_started_rx) = oneshot::channel();
    let (operation_finish_tx, operation_finish_rx) = oneshot::channel();
    let (cancel_started_tx, cancel_started_rx) = oneshot::channel();
    let (cancel_finish_tx, cancel_finish_rx) = oneshot::channel();
    let cancellation_dispatched = Arc::new(AtomicBool::new(false));
    let cancellation_dispatched_worker = Arc::clone(&cancellation_dispatched);
    let operation = async move {
        let _ = operation_started_tx.send(());
        operation_finish_rx
            .await
            .map_err(|_| DriverError::Internal("operation completion signal was dropped".into()))?
    };
    let cancellation = async move {
        let _ = cancel_started_tx.send(());
        let _ = cancel_finish_rx.await;
        cancellation_dispatched_worker.store(true, Ordering::SeqCst);
        Ok::<_, DriverError>(())
    };
    let task = tokio::spawn(async move { run_server_cancellable(operation, cancellation, |_| false, &control).await });

    operation_started_rx.await.expect("operation should start");
    token.cancel();
    cancel_started_rx.await.expect("cancel dispatch should start");
    operation_finish_tx
        .send(Ok::<_, DriverError>(7))
        .expect("operation should finish");
    tokio::task::yield_now().await;

    assert!(!task.is_finished());
    cancel_finish_tx.send(()).expect("cancel dispatch should finish");

    let error = task
        .await
        .expect("task should finish")
        .expect_err("a completed result after cancellation is uncertain");
    assert!(matches!(
        error,
        DriverError::OperationOutcomeUnknown { source }
            if matches!(source.as_ref(), DriverError::Cancelled)
    ));
    assert!(cancellation_dispatched.load(Ordering::SeqCst));
}

#[tokio::test]
async fn a_stop_the_operation_noticed_itself_is_reported_as_a_plain_cancellation() {
    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let stopper = token.clone();
    let operation = async move {
        stopper.cancel();
        Err::<(), _>(DriverError::Transaction {
            statement_index: 3,
            source: Box::new(DriverError::Cancelled),
        })
    };
    let confirms = |error: &DriverError| matches!(error, DriverError::Transaction { source, .. } if matches!(source.as_ref(), DriverError::Cancelled));

    let result = run_server_cancellable(operation, async { Ok(()) }, confirms, &control).await;

    assert!(matches!(result, Err(DriverError::Cancelled)), "{result:?}");
}

fn confirms_test_cancellation(error: &DriverError) -> bool {
    match error {
        DriverError::Query { .. } => true,
        DriverError::Transaction { source, .. } => matches!(source.as_ref(), DriverError::Query { .. }),
        _ => false,
    }
}

async fn run_after_server_cancel(
    control: OperationControl,
    cancel_token: Option<CancellationToken>,
    operation_error: DriverError,
) -> Result<(), DriverError> {
    use tokio::sync::oneshot;

    let (started_tx, started_rx) = oneshot::channel();
    let (stopped_tx, stopped_rx) = oneshot::channel();
    let operation = async move {
        let _ = started_tx.send(());
        stopped_rx
            .await
            .map_err(|_| DriverError::Internal("server stop signal was dropped".into()))?;
        Err(operation_error)
    };
    let cancellation = async move {
        stopped_tx
            .send(())
            .map_err(|_| DriverError::Internal("operation stop receiver was dropped".into()))
    };
    let task = tokio::spawn(async move {
        run_server_cancellable_preserving_transaction_rollback(
            operation,
            cancellation,
            confirms_test_cancellation,
            &control,
        )
        .await
    });

    started_rx.await.expect("operation should start");
    if let Some(token) = cancel_token {
        token.cancel();
    }
    task.await.expect("operation task should finish")
}

#[tokio::test]
async fn server_confirmed_transaction_cancellation_preserves_rollback_and_cancelled_status() {
    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let error = DriverError::Transaction {
        statement_index: 4,
        source: Box::new(DriverError::query("server cancelled the statement")),
    };

    let result = run_after_server_cancel(control, Some(token), error).await;

    assert!(
        matches!(
            &result,
            Err(DriverError::Transaction {
                statement_index: 4,
                source
            }) if matches!(source.as_ref(), DriverError::Cancelled)
        ),
        "{result:?}"
    );
}

#[tokio::test]
async fn server_confirmed_transaction_timeout_preserves_rollback_and_timeout_status() {
    let deadline = Instant::now() + Duration::from_millis(10);
    let control = OperationControl::new(CancellationToken::new(), Some(deadline));
    let error = DriverError::Transaction {
        statement_index: 4,
        source: Box::new(DriverError::query("server cancelled the statement")),
    };

    let result = run_after_server_cancel(control, None, error).await;

    assert!(
        matches!(
            &result,
            Err(DriverError::Transaction {
                statement_index: 4,
                source
            }) if matches!(source.as_ref(), DriverError::TimedOut)
        ),
        "{result:?}"
    );
}

#[tokio::test]
async fn server_confirmed_non_transaction_cancellation_remains_plain() {
    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let error = DriverError::query("server cancelled the statement");

    let result = run_after_server_cancel(control, Some(token), error).await;

    assert!(matches!(&result, Err(DriverError::Cancelled)), "{result:?}");
}

#[tokio::test]
async fn transaction_cancellation_before_dispatch_remains_plain() {
    let token = CancellationToken::new();
    token.cancel();
    let control = OperationControl::new(token, None);
    let error = DriverError::Transaction {
        statement_index: 4,
        source: Box::new(DriverError::query("server cancelled the statement")),
    };

    let result = run_server_cancellable_preserving_transaction_rollback(
        async { Err::<(), _>(error) },
        async { Ok(()) },
        confirms_test_cancellation,
        &control,
    )
    .await;

    assert!(matches!(&result, Err(DriverError::Cancelled)), "{result:?}");
}

#[tokio::test]
async fn unconfirmed_rollback_after_cancellation_remains_unknown() {
    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let error = DriverError::TransactionRollbackFailed {
        statement_index: 4,
        source: Box::new(DriverError::query("server cancelled the statement")),
        rollback_error: Box::new(DriverError::Disconnected),
    };

    let result = run_after_server_cancel(control, Some(token), error).await;

    assert!(
        matches!(
            &result,
            Err(DriverError::OperationOutcomeUnknown { source })
                if matches!(source.as_ref(), DriverError::TransactionRollbackFailed { .. })
        ),
        "{result:?}"
    );
}
