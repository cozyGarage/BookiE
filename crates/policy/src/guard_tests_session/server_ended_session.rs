use super::*;

async fn written_transaction_then(h: &Harness, last: &str) -> Result<QueryResult, DriverError> {
    let mut session = h.guard.open_session().await.expect("open session");
    run(&mut session, "BEGIN").await.expect("begin");
    run(&mut session, "UPDATE items SET v = 1 WHERE id = 1")
        .await
        .expect("write");
    let result = run(&mut session, last).await;
    let _ = session.close().await;
    result
}

fn rollback_outcomes(h: &Harness) -> Vec<AuditEvent> {
    outcomes_of(&h.audit, AuditOperationClass::TransactionRollback)
}

#[tokio::test]
async fn a_server_ended_session_inside_a_written_transaction_records_a_rollback_and_keeps_writes() {
    let h = harness(
        SessionScript {
            server_ended_on: Some("SELECT 1"),
            ..SessionScript::default()
        },
        false,
    );

    let result = written_transaction_then(&h, "SELECT 1").await;

    assert!(
        matches!(result, Err(DriverError::SessionEndedByServer { .. })),
        "{result:?}"
    );
    assert!(
        !h.audit_state.governed_writes_disabled(),
        "nothing was committed, so writes stay available"
    );
    let rollbacks = rollback_outcomes(&h);
    assert_eq!(rollbacks.len(), 1, "{rollbacks:?}");
    assert_eq!(rollbacks[0].transaction_outcome, AuditTransactionOutcome::RolledBack);
    assert_ne!(rollbacks[0].terminal_status, AuditTerminalStatus::Unknown);
}

#[tokio::test]
async fn a_network_loss_inside_a_written_transaction_still_blocks_writes() {
    let h = harness(
        SessionScript {
            disconnect_on: Some("SELECT 1"),
            ..SessionScript::default()
        },
        false,
    );

    let _ = written_transaction_then(&h, "SELECT 1").await;

    assert!(h.audit_state.governed_writes_disabled());
    assert!(
        rollback_outcomes(&h)
            .iter()
            .any(|event| event.transaction_outcome == AuditTransactionOutcome::Unknown),
        "{:?}",
        rollback_outcomes(&h)
    );
}

#[tokio::test]
async fn a_server_ending_the_session_during_commit_stays_unknown() {
    let h = harness(
        SessionScript {
            server_ended_on: Some("COMMIT"),
            ..SessionScript::default()
        },
        false,
    );

    let result = written_transaction_then(&h, "COMMIT").await;

    assert!(result.is_err());
    assert!(h.audit_state.governed_writes_disabled(), "the commit may have landed");
    let commits = outcomes_of(&h.audit, AuditOperationClass::TransactionCommit);
    assert!(
        commits
            .iter()
            .all(|event| event.transaction_outcome == AuditTransactionOutcome::Unknown),
        "{commits:?}"
    );
}
