use super::*;

#[derive(Default)]
struct SessionScript {
    sent: Mutex<Vec<String>>,
    lose_on: Option<&'static str>,
}

struct ScriptedSession {
    script: Arc<SessionScript>,
    usable: bool,
}

#[async_trait]
impl tablepro_core::Session for ScriptedSession {
    async fn query_params_controlled(
        &mut self,
        sql: &str,
        _params: &[Value],
        _control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        self.script.sent.lock().expect("sent lock").push(sql.to_string());
        if self.script.lose_on == Some(sql) {
            self.usable = false;
            return Err(DriverError::Cancelled);
        }
        Ok(empty_result())
    }

    fn is_usable(&self) -> bool {
        self.usable
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct SessionConn {
    script: Arc<SessionScript>,
}

#[async_trait]
impl Connection for SessionConn {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(vec![])
    }

    async fn fetch_columns(&self, _: Option<&str>, _: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        Ok(vec![])
    }

    async fn fetch_rows(&self, _: Option<&str>, _: &str, _: u64, _: u64) -> Result<QueryResult, DriverError> {
        Ok(empty_result())
    }

    async fn query(&self, _: &str) -> Result<QueryResult, DriverError> {
        Ok(empty_result())
    }

    async fn execute(&self, _: &str) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 0 })
    }

    async fn execute_params(&self, _: &str, _: &[Value]) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 0 })
    }

    async fn execute_in_transaction(&self, _: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Ok(vec![])
    }

    async fn open_session(&self) -> Result<Box<dyn tablepro_core::Session>, DriverError> {
        Ok(Box::new(ScriptedSession {
            script: self.script.clone(),
            usable: true,
        }))
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct Harness {
    guard: PolicyGuard,
    script: Arc<SessionScript>,
    audit: Arc<SequenceAuditSink>,
    audit_state: Arc<AuditState>,
}

fn harness(script: SessionScript, read_only: bool) -> Harness {
    let script = Arc::new(script);
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let audit_state = Arc::new(AuditState::new());
    let mut ctx = context(
        Principal::human_gui(),
        Environment::Local,
        PolicyConfig::default(),
        Arc::new(AutoApproveSink),
        audit.clone(),
        audit_state.clone(),
    );
    ctx.read_only = read_only;
    let inner: Arc<dyn Connection> = Arc::new(SessionConn { script: script.clone() });
    Harness {
        guard: PolicyGuard::new(inner, ctx),
        script,
        audit,
        audit_state,
    }
}

async fn run(session: &mut Box<dyn tablepro_core::Session>, sql: &str) -> Result<QueryResult, DriverError> {
    session
        .query_params_controlled(sql, &[], &OperationControl::with_timeout(Duration::from_secs(5)))
        .await
}

fn outcomes_of(audit: &SequenceAuditSink, class: AuditOperationClass) -> Vec<AuditEvent> {
    audit
        .events
        .lock()
        .expect("event lock")
        .iter()
        .filter(|event| event.operation_class == class && event.phase == AuditRecordPhase::Outcome)
        .cloned()
        .collect()
}

#[tokio::test]
async fn a_session_runs_its_own_transaction_and_audits_the_rollback_against_its_batch() {
    let h = harness(SessionScript::default(), false);
    let mut session = h.guard.open_session().await.expect("open session");

    run(&mut session, "BEGIN").await.expect("begin is allowed in a session");
    assert!(session.transaction_open());
    run(&mut session, "UPDATE items SET v = 1 WHERE id = 1")
        .await
        .expect("write");
    run(&mut session, "ROLLBACK TO SAVEPOINT s")
        .await
        .expect("savepoint rollback stays inside");
    assert!(session.transaction_open());
    run(&mut session, "ROLLBACK").await.expect("rollback");
    assert!(!session.transaction_open());

    let rollbacks = outcomes_of(&h.audit, AuditOperationClass::TransactionRollback);
    assert_eq!(rollbacks.len(), 1);
    assert_eq!(rollbacks[0].transaction_outcome, AuditTransactionOutcome::RolledBack);
    let events = h.audit.events.lock().expect("event lock");
    let write = events
        .iter()
        .find(|event| {
            event.operation_class.is_write() && event.operation_class != AuditOperationClass::TransactionRollback
        })
        .expect("the write is audited");
    assert_eq!(write.batch_id, rollbacks[0].batch_id);
    assert!(write.batch_id.is_some());
    assert_eq!(
        *h.script.sent.lock().expect("sent lock"),
        vec![
            "BEGIN",
            "UPDATE items SET v = 1 WHERE id = 1",
            "ROLLBACK TO SAVEPOINT s",
            "ROLLBACK"
        ]
    );
}

#[tokio::test]
async fn a_committed_session_transaction_records_the_commit() {
    let h = harness(SessionScript::default(), false);
    let mut session = h.guard.open_session().await.expect("open session");

    for sql in ["BEGIN", "INSERT INTO items VALUES (1)", "COMMIT"] {
        run(&mut session, sql).await.expect(sql);
    }

    let commits = outcomes_of(&h.audit, AuditOperationClass::TransactionCommit);
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].transaction_outcome, AuditTransactionOutcome::Committed);
    assert!(!session.transaction_open());
}

#[tokio::test]
async fn policy_still_denies_a_write_inside_a_session_on_a_read_only_connection() {
    let h = harness(SessionScript::default(), true);
    let mut session = h.guard.open_session().await.expect("open session");

    run(&mut session, "BEGIN").await.expect("begin");
    let error = run(&mut session, "DELETE FROM items WHERE id = 1")
        .await
        .expect_err("read-only must deny the write");

    assert!(
        matches!(error, DriverError::PolicyDenied(_) | DriverError::ReadOnly),
        "{error:?}"
    );
    assert!(
        !h.script
            .sent
            .lock()
            .expect("sent lock")
            .iter()
            .any(|sql| sql.starts_with("DELETE"))
    );
}

#[tokio::test]
async fn closing_a_session_with_an_open_transaction_rolls_it_back_and_audits_it() {
    let h = harness(SessionScript::default(), false);
    let mut session = h.guard.open_session().await.expect("open session");
    run(&mut session, "BEGIN").await.expect("begin");
    run(&mut session, "UPDATE items SET v = 2 WHERE id = 1")
        .await
        .expect("write");

    session.close().await.expect("close");

    assert_eq!(
        h.script.sent.lock().expect("sent lock").last().map(String::as_str),
        Some("ROLLBACK")
    );
    let rollbacks = outcomes_of(&h.audit, AuditOperationClass::TransactionRollback);
    assert_eq!(rollbacks.len(), 1);
    assert_eq!(rollbacks[0].transaction_outcome, AuditTransactionOutcome::RolledBack);
    assert!(!h.audit_state.governed_writes_disabled());
}

#[tokio::test]
async fn a_session_lost_mid_transaction_after_a_write_records_an_unknown_outcome() {
    let h = harness(
        SessionScript {
            lose_on: Some("SELECT pg_sleep(60)"),
            ..SessionScript::default()
        },
        false,
    );
    let mut session = h.guard.open_session().await.expect("open session");
    run(&mut session, "BEGIN").await.expect("begin");
    run(&mut session, "UPDATE items SET v = 3 WHERE id = 1")
        .await
        .expect("write");
    let _ = run(&mut session, "SELECT pg_sleep(60)").await;
    assert!(!session.is_usable());

    session.close().await.expect("close");

    let rollbacks = outcomes_of(&h.audit, AuditOperationClass::TransactionRollback);
    assert_eq!(rollbacks.len(), 1);
    assert_eq!(rollbacks[0].transaction_outcome, AuditTransactionOutcome::Unknown);
    assert!(h.audit_state.governed_writes_disabled());
    assert_ne!(
        h.script.sent.lock().expect("sent lock").last().map(String::as_str),
        Some("ROLLBACK")
    );
}
