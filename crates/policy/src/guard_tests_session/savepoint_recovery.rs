use super::*;

#[tokio::test]
async fn committing_after_confirmed_savepoint_recovery_is_audited_as_committed() {
    for (savepoint, recovery) in [
        ("SAVEPOINT recovery", "ROLLBACK TO SAVEPOINT recovery"),
        ("SAVEPOINT recovery", "ROLLBACK TO recovery"),
        (
            "SAVEPOINT \"Recovery Point\"",
            "/* recovery */ ROLLBACK TO SAVEPOINT \"Recovery Point\"",
        ),
    ] {
        let h = harness(
            SessionScript {
                query_error_on: Some("SELECT 1 / 0"),
                ..SessionScript::default()
            },
            false,
        );
        let mut session = h.guard.open_session().await.expect("open session");
        run(&mut session, "BEGIN").await.expect("begin");
        run(&mut session, savepoint).await.expect("savepoint");
        run(&mut session, "SELECT 1 / 0").await.expect_err("statement fails");
        run(&mut session, recovery).await.expect("recover savepoint");
        run(&mut session, "COMMIT").await.expect("commit");

        let commits = outcomes_of(&h.audit, AuditOperationClass::TransactionCommit);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].transaction_outcome, AuditTransactionOutcome::Committed);
        assert!(!session.transaction_open());
    }
}

#[tokio::test]
async fn a_failed_savepoint_recovery_keeps_the_commit_audited_as_rolled_back() {
    let h = harness(
        SessionScript {
            fail_once_on: Some("SELECT pg_sleep(1)"),
            query_error_on: Some("ROLLBACK TO SAVEPOINT missing"),
            ..SessionScript::default()
        },
        false,
    );
    let mut session = h.guard.open_session().await.expect("open session");
    run(&mut session, "BEGIN").await.expect("begin");
    run(&mut session, "SELECT pg_sleep(1)")
        .await
        .expect_err("statement fails");
    run(&mut session, "ROLLBACK TO SAVEPOINT missing")
        .await
        .expect_err("recovery fails");
    run(&mut session, "COMMIT").await.expect("commit command succeeds");

    let commits = outcomes_of(&h.audit, AuditOperationClass::TransactionCommit);
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].transaction_outcome, AuditTransactionOutcome::RolledBack);
}

#[tokio::test]
async fn savepoint_words_in_a_value_do_not_clear_an_aborted_transaction() {
    let h = harness(
        SessionScript {
            query_error_on: Some("SELECT 1 / 0"),
            ..SessionScript::default()
        },
        false,
    );
    let mut session = h.guard.open_session().await.expect("open session");
    run(&mut session, "BEGIN").await.expect("begin");
    run(&mut session, "SELECT 1 / 0").await.expect_err("statement fails");
    run(&mut session, "SELECT 'ROLLBACK TO SAVEPOINT recovery'")
        .await
        .expect("scripted successful result");
    run(&mut session, "COMMIT").await.expect("commit command succeeds");

    let commits = outcomes_of(&h.audit, AuditOperationClass::TransactionCommit);
    assert_eq!(commits[0].transaction_outcome, AuditTransactionOutcome::RolledBack);
}
