use super::*;
use tablepro_core::Session;

#[tokio::test]
async fn a_read_only_connection_can_open_a_postgres_read_only_session() {
    let h = harness(SessionScript::default(), true);
    let mut session = h.guard.open_session().await.expect("open session");

    run(&mut session, "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .expect("read-only connection can start its read snapshot");
    run(&mut session, "SELECT 1")
        .await
        .expect("read-only snapshot permits reads");
    run(&mut session, "ROLLBACK")
        .await
        .expect("read-only snapshot can roll back");
}

#[tokio::test]
async fn session_completion_metadata_is_a_governed_and_audited_read() {
    let h = harness(SessionScript::default(), true);
    let mut session = h.guard.open_session().await.expect("open session");
    let columns = session
        .fetch_completion_columns_controlled(
            Some("public"),
            "items",
            &OperationControl::with_timeout(Duration::from_secs(1)),
        )
        .await
        .expect("completion metadata is available through the guarded session");

    assert_eq!(columns, ["id"]);
    assert_eq!(
        *h.script.completion_targets.lock().expect("target lock"),
        vec![(Some("public".into()), "items".into())]
    );
    let events = h.audit.events.lock().expect("events");
    let outcome = events
        .iter()
        .find(|event| event.phase == AuditRecordPhase::Outcome)
        .expect("metadata outcome is audited");
    assert_eq!(outcome.operation_class, AuditOperationClass::Read);
    assert_eq!(outcome.decision_rule, "metadata_read");
    assert_eq!(outcome.terminal_status, AuditTerminalStatus::Succeeded);
    assert_eq!(outcome.rows_affected, Some(1));
}

#[tokio::test]
async fn audited_session_forwards_completion_metadata_and_records_the_outcome() {
    let h = harness(SessionScript::default(), true);
    let mut session = h.guard.open_session_with_audit().await.expect("open audited session");
    let columns = session
        .fetch_completion_columns_controlled(
            Some("public"),
            "items",
            &OperationControl::with_timeout(Duration::from_secs(1)),
        )
        .await
        .expect("completion metadata is available through the audited session");

    assert_eq!(columns, ["id"]);
    assert_eq!(
        *h.script.completion_targets.lock().expect("target lock"),
        vec![(Some("public".into()), "items".into())]
    );
    let events = h.audit.events.lock().expect("events");
    let outcome = events
        .iter()
        .find(|event| event.phase == AuditRecordPhase::Outcome)
        .expect("metadata outcome is audited");
    assert_eq!(outcome.operation_class, AuditOperationClass::Read);
    assert_eq!(outcome.decision_rule, "metadata_read");
    assert_eq!(outcome.terminal_status, AuditTerminalStatus::Succeeded);
    assert_eq!(outcome.rows_affected, Some(1));
}

#[tokio::test]
async fn audited_session_completion_metadata_preserves_timeout_and_audit_outcome() {
    let h = harness(
        SessionScript {
            completion_timed_out: true,
            ..SessionScript::default()
        },
        false,
    );
    let mut session = h.guard.open_session_with_audit().await.expect("open audited session");
    let error = session
        .fetch_completion_columns_controlled(None, "items", &OperationControl::with_timeout(Duration::from_secs(1)))
        .await
        .expect_err("the timed out metadata read remains a timeout");

    assert!(matches!(error, DriverError::TimedOut));
    let events = h.audit.events.lock().expect("events");
    let outcome = events
        .iter()
        .find(|event| event.phase == AuditRecordPhase::Outcome)
        .expect("timeout outcome is audited");
    assert_eq!(outcome.terminal_status, AuditTerminalStatus::TimedOut);
}

#[tokio::test]
async fn audited_session_completion_metadata_respects_pre_dispatch_cancellation() {
    let h = harness(SessionScript::default(), false);
    let mut session = h.guard.open_session_with_audit().await.expect("open audited session");
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let error = session
        .fetch_completion_columns_controlled(None, "items", &OperationControl::new(cancellation, None))
        .await
        .expect_err("a cancelled metadata request is stopped before dispatch");

    assert!(matches!(error, DriverError::Cancelled));
    assert!(
        h.script
            .completion_targets
            .lock()
            .expect("completion target lock")
            .is_empty()
    );
}

#[tokio::test]
async fn session_completion_metadata_records_a_timed_out_terminal_outcome() {
    let h = harness(
        SessionScript {
            completion_timed_out: true,
            ..SessionScript::default()
        },
        false,
    );
    let mut session = h.guard.open_session().await.expect("open session");
    let error = session
        .fetch_completion_columns_controlled(None, "items", &OperationControl::with_timeout(Duration::from_secs(1)))
        .await
        .expect_err("the scripted session times out");

    assert!(matches!(error, DriverError::TimedOut));
    let events = h.audit.events.lock().expect("events");
    let outcome = events
        .iter()
        .find(|event| event.phase == AuditRecordPhase::Outcome)
        .expect("timeout outcome is audited");
    assert_eq!(outcome.terminal_status, AuditTerminalStatus::TimedOut);
}

#[tokio::test]
async fn cancelled_session_completion_metadata_is_not_dispatched() {
    let h = harness(SessionScript::default(), false);
    let mut session = h.guard.open_session().await.expect("open session");
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let error = session
        .fetch_completion_columns_controlled(None, "items", &OperationControl::new(cancellation, None))
        .await
        .expect_err("a cancelled metadata request is stopped before dispatch");

    assert!(matches!(error, DriverError::Cancelled));
    assert!(
        h.script
            .completion_targets
            .lock()
            .expect("completion target lock")
            .is_empty()
    );
}

#[tokio::test]
async fn a_retired_session_refuses_completion_metadata_before_driver_dispatch() {
    let h = harness(
        SessionScript {
            lose_on: Some("SELECT 1"),
            ..SessionScript::default()
        },
        false,
    );
    let mut session = h.guard.open_session().await.expect("open session");
    run(&mut session, "SELECT 1")
        .await
        .expect_err("the session connection is lost");
    let error = session
        .fetch_completion_columns_controlled(None, "items", &OperationControl::with_timeout(Duration::from_secs(1)))
        .await
        .expect_err("a retired session cannot serve completion metadata");

    assert!(matches!(error, DriverError::PolicyDenied(_)), "{error:?}");
    assert!(
        h.script
            .completion_targets
            .lock()
            .expect("completion target lock")
            .is_empty()
    );
}

#[tokio::test]
async fn a_read_only_connection_refuses_and_audits_a_read_write_begin() {
    let h = harness(SessionScript::default(), true);
    let mut session = h.guard.open_session().await.expect("open session");

    let error = run(&mut session, "BEGIN READ WRITE")
        .await
        .expect_err("read-only connection must refuse a read-write transaction");

    assert!(matches!(error, DriverError::PolicyDenied(_)), "{error:?}");
    assert!(!session.transaction_open());
    assert!(h.script.sent.lock().expect("sent lock").is_empty());
    let events = h.audit.events.lock().expect("event lock");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].phase, AuditRecordPhase::Outcome);
    assert_eq!(events[0].terminal_status, AuditTerminalStatus::Denied);
}
