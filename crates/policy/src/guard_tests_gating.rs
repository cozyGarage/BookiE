use sha2::Digest;

use super::*;

#[tokio::test]
async fn categorical_agent_denial_skips_blast_radius_query() {
    let queries = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        query_counting_connection(queries.clone()),
        context(
            Principal::Agent {
                token: "token".into(),
                client: None,
                model: None,
            },
            Environment::Prod,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    );

    guard
        .execute("UPDATE jobs SET status = 'done' WHERE id = 1")
        .await
        .expect_err("agent write must be denied");

    assert_eq!(queries.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn read_only_denial_skips_blast_radius_query() {
    let queries = Arc::new(AtomicUsize::new(0));
    let mut guard_context = context(
        Principal::human_gui(),
        Environment::Prod,
        PolicyConfig::default(),
        Arc::new(AutoApproveSink),
        Arc::new(SequenceAuditSink::new(vec![])),
        Arc::new(AuditState::new()),
    );
    guard_context.read_only = true;
    let guard = PolicyGuard::new(query_counting_connection(queries.clone()), guard_context);

    guard
        .execute("UPDATE jobs SET status = 'done' WHERE id = 1")
        .await
        .expect_err("read-only write must be denied");

    assert_eq!(queries.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn mysql_mode_dependent_statements_are_denied_and_audited_before_dispatch() {
    let queries = Arc::new(AtomicUsize::new(0));
    let approvals = Arc::new(AtomicUsize::new(0));
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let mut guard_context = context(
        Principal::human_gui(),
        Environment::Local,
        PolicyConfig::default(),
        Arc::new(CountingApprovalSink {
            calls: approvals.clone(),
        }),
        audit.clone(),
        Arc::new(AuditState::new()),
    );
    guard_context.driver_id = "mysql".into();
    let guard = PolicyGuard::new(query_counting_connection(queries.clone()), guard_context);

    let error = guard
        .query(r#"SELECT 'x\'; DELETE FROM users WHERE id = 1 -- '"#)
        .await
        .expect_err("a session-mode-dependent statement must be refused");

    assert!(matches!(error, DriverError::PolicyDenied(_)), "{error:?}");
    assert_eq!(queries.load(Ordering::SeqCst), 0);
    assert_eq!(approvals.load(Ordering::SeqCst), 0);
    let events = audit.events.lock().expect("audit events");
    assert!(events.iter().any(|event| {
        event.phase == crate::audit::AuditRecordPhase::Outcome
            && event.terminal_status == crate::audit::AuditTerminalStatus::Denied
            && event.error_category == Some(crate::audit::AuditErrorCategory::Policy)
    }));
}

#[tokio::test]
async fn dangerous_effects_ask_for_approval_without_running_a_blast_radius_query() {
    let queries = Arc::new(AtomicUsize::new(0));
    let approvals = Arc::new(AtomicUsize::new(0));
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = PolicyGuard::new(
        query_counting_connection(queries.clone()),
        context(
            Principal::human_gui(),
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(CountingApprovalSink {
                calls: approvals.clone(),
            }),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );

    guard
        .execute("COPY items TO PROGRAM 'echo unsafe'")
        .await
        .expect("the mocked approval sink approves the administrative statement");

    assert_eq!(approvals.load(Ordering::SeqCst), 1);
    assert_eq!(queries.load(Ordering::SeqCst), 0);
    let events = audit.events.lock().expect("audit events");
    assert!(events.iter().any(|event| {
        event
            .decision_rule
            .trim_end_matches(":approved")
            .split('+')
            .any(|rule| rule == "human_dangerous_effect_approval")
    }));
}

fn mssql_guard(principal: Principal, approval: Arc<dyn ApprovalSink>, queries: Arc<AtomicUsize>) -> PolicyGuard {
    let mut guard_context = context(
        principal,
        Environment::Local,
        PolicyConfig::default(),
        approval,
        Arc::new(SequenceAuditSink::new(vec![])),
        Arc::new(AuditState::new()),
    );
    guard_context.driver_id = "mssql".into();
    PolicyGuard::new(query_counting_connection(queries), guard_context)
}

#[tokio::test]
async fn a_linked_server_read_asks_for_approval_for_a_human_and_for_an_agent_before_it_runs() {
    for principal in [
        Principal::human_gui(),
        Principal::Agent {
            token: "token".into(),
            client: None,
            model: None,
        },
    ] {
        let queries = Arc::new(AtomicUsize::new(0));
        let approvals = Arc::new(AtomicUsize::new(0));
        let guard = mssql_guard(
            principal.clone(),
            Arc::new(CountingApprovalSink {
                calls: approvals.clone(),
            }),
            queries.clone(),
        );

        guard
            .query("SELECT * FROM linked.db.dbo.t")
            .await
            .expect("the mocked approval sink approves the linked-server read");

        assert_eq!(approvals.load(Ordering::SeqCst), 1, "{principal:?}");
        assert_eq!(queries.load(Ordering::SeqCst), 1, "{principal:?}");
    }
}

#[tokio::test]
async fn a_refused_linked_server_read_never_reaches_the_driver() {
    let queries = Arc::new(AtomicUsize::new(0));
    let guard = mssql_guard(Principal::human_gui(), Arc::new(DenyApprovalSink), queries.clone());

    let error = guard
        .query("SELECT * FROM linked.db.dbo.t")
        .await
        .expect_err("a denied approval must stop the read");

    assert!(matches!(error, DriverError::PolicyDenied(_)), "{error:?}");
    assert_eq!(queries.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_read_of_a_local_sql_server_table_does_not_ask_for_approval() {
    let queries = Arc::new(AtomicUsize::new(0));
    let approvals = Arc::new(AtomicUsize::new(0));
    let guard = mssql_guard(
        Principal::human_gui(),
        Arc::new(CountingApprovalSink {
            calls: approvals.clone(),
        }),
        queries.clone(),
    );

    guard.query("SELECT * FROM db.dbo.t").await.expect("an ordinary read");

    assert_eq!(approvals.load(Ordering::SeqCst), 0);
    assert_eq!(queries.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn oracle_stored_function_is_denied_before_driver_dispatch_without_approval() {
    let queries = Arc::new(AtomicUsize::new(0));
    let mut guard_context = context(
        Principal::human_gui(),
        Environment::Local,
        PolicyConfig::default(),
        Arc::new(DenyApprovalSink),
        Arc::new(SequenceAuditSink::new(vec![])),
        Arc::new(AuditState::new()),
    );
    guard_context.driver_id = "oracle".into();
    let guard = PolicyGuard::new(query_counting_connection(queries.clone()), guard_context);

    let error = guard
        .query("SELECT bookie_it.delayed_write() FROM dual")
        .await
        .expect_err("stored function must need approval");

    assert!(matches!(error, DriverError::PolicyDenied(_)), "{error:?}");
    assert_eq!(queries.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn write_approval_overrides_the_human_unparseable_allow_rule() {
    let approvals = Arc::new(AtomicUsize::new(0));
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let mut policy = PolicyConfig::default();
    let local = policy.environments.entry("local".into()).or_default();
    local.human_approve_writes = Some(true);
    local.human_approve_unparseable = Some(false);
    let guard = PolicyGuard::new(
        connection(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Local,
            policy,
            Arc::new(CountingApprovalSink {
                calls: approvals.clone(),
            }),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );

    guard
        .execute("DELETE FROM items WHERE id = 1; SELECT (")
        .await
        .expect("the mocked approval sink approves the malformed script");

    assert_eq!(approvals.load(Ordering::SeqCst), 1);
    let events = audit.events.lock().expect("audit events");
    assert!(events.iter().any(|event| {
        event
            .decision_rule
            .trim_end_matches(":approved")
            .split('+')
            .any(|rule| rule == "human_write_approve")
    }));
}

#[tokio::test]
async fn unknown_effects_require_approval_when_unparseable_human_sql_is_allowed() {
    let approvals = Arc::new(AtomicUsize::new(0));
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let mut policy = PolicyConfig::default();
    let local = policy.environments.entry("local".into()).or_default();
    local.human_approve_writes = Some(false);
    local.human_approve_unparseable = Some(false);
    let guard = PolicyGuard::new(
        connection(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Local,
            policy,
            Arc::new(CountingApprovalSink {
                calls: approvals.clone(),
            }),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );

    guard
        .execute("DELETE FROM items WHERE id = 1; SELECT (")
        .await
        .expect("the mocked approval sink approves the malformed script");

    assert_eq!(approvals.load(Ordering::SeqCst), 1);
    let events = audit.events.lock().expect("audit events");
    assert!(events.iter().any(|event| {
        event
            .decision_rule
            .trim_end_matches(":approved")
            .split('+')
            .any(|rule| rule == "human_dangerous_effect_approval")
    }));
}

#[tokio::test]
async fn implicit_transaction_mode_is_denied_before_dispatch_and_audited() {
    let executes = Arc::new(AtomicUsize::new(0));
    let approvals = Arc::new(AtomicUsize::new(0));
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let mut guard_context = context(
        Principal::human_gui(),
        Environment::Prod,
        PolicyConfig::default(),
        Arc::new(CountingApprovalSink {
            calls: approvals.clone(),
        }),
        audit.clone(),
        Arc::new(AuditState::new()),
    );
    guard_context.driver_id = "mysql".into();
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        guard_context,
    );

    guard
        .execute("SET autocommit = 0")
        .await
        .expect_err("shared connection must refuse implicit transaction mode");

    assert_eq!(executes.load(Ordering::SeqCst), 0);
    assert_eq!(approvals.load(Ordering::SeqCst), 0);
    let events = audit.events.lock().expect("audit events");
    let event = events.last().expect("denial outcome");
    assert_eq!(event.decision_rule, "transaction_control_needs_session");
    assert_eq!(event.terminal_status, AuditTerminalStatus::Denied);
}

#[tokio::test]
async fn disabled_audit_state_cannot_be_bypassed_by_approval() {
    let queries = Arc::new(AtomicUsize::new(0));
    let approvals = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        query_counting_connection(queries.clone()),
        context(
            Principal::human_gui(),
            Environment::Prod,
            PolicyConfig::default(),
            Arc::new(CountingApprovalSink {
                calls: approvals.clone(),
            }),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::with_governed_writes_disabled()),
        ),
    );

    guard
        .execute("UPDATE jobs SET status = 'done' WHERE id = 1")
        .await
        .expect_err("disabled audit state must deny the write");

    assert_eq!(approvals.load(Ordering::SeqCst), 0);
    assert_eq!(queries.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn agent_transaction_read_intent_failure_prevents_driver_query() {
    let queries = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        query_counting_connection(queries.clone()),
        context(
            Principal::Agent {
                token: "token".into(),
                client: None,
                model: None,
            },
            Environment::Local,
            allowed_agent_policy(),
            Arc::new(AutoApproveSink),
            Arc::new(SequenceAuditSink::new(vec![AuditRecordPhase::Intent])),
            Arc::new(AuditState::new()),
        ),
    );
    let mut transaction = guard.begin().await.expect("begin");

    transaction
        .query("SELECT 1")
        .await
        .expect_err("read intent failure must surface");

    assert_eq!(queries.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn required_intent_failure_prevents_driver_execution() {
    let executes = Arc::new(AtomicUsize::new(0));
    let audit = Arc::new(SequenceAuditSink::new(vec![AuditRecordPhase::Intent]));
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Prod,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            audit,
            Arc::new(AuditState::new()),
        ),
    );

    let error = guard
        .execute("INSERT INTO jobs(id) VALUES (1)")
        .await
        .expect_err("intent must fail");

    assert!(
        matches!(&error, DriverError::PolicyDenied(message) if message.contains("audit intent could not be persisted"))
    );
    assert_eq!(executes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn commit_intent_failure_prevents_inner_commit() {
    let commits = Arc::new(AtomicUsize::new(0));
    let audit = Arc::new(SequenceAuditSink::new(vec![AuditRecordPhase::Intent]));
    let guard = PolicyGuard::new(
        connection(Arc::new(AtomicUsize::new(0)), commits.clone()),
        context(
            Principal::human_gui(),
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            audit,
            Arc::new(AuditState::new()),
        ),
    );
    let transaction = guard.begin().await.expect("begin");

    let error = transaction.commit().await.expect_err("commit intent must fail");

    assert!(
        matches!(&error, DriverError::PolicyDenied(message) if message.contains("audit intent could not be persisted"))
    );
    assert_eq!(commits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn rollback_uses_distinct_operation_class() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = PolicyGuard::new(
        connection(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Prod,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );
    let transaction = guard.begin().await.expect("begin");

    transaction.rollback().await.expect("rollback");

    let events = audit.events.lock().expect("event lock");
    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .all(|event| event.operation_class == AuditOperationClass::TransactionRollback)
    );
    assert_eq!(events[0].phase, AuditRecordPhase::Intent);
    assert_eq!(events[1].transaction_outcome, AuditTransactionOutcome::RolledBack);
}

#[tokio::test]
async fn approve_once_authorizes_only_the_current_operation() {
    let executes = Arc::new(AtomicUsize::new(0));
    let approvals = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Prod,
            PolicyConfig::default(),
            Arc::new(SequenceApprovalSink {
                calls: approvals.clone(),
                outcomes: Mutex::new(vec![ApprovalOutcome::AllowOnce, ApprovalOutcome::Deny]),
            }),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    );

    guard
        .execute("INSERT INTO jobs(id) VALUES (1)")
        .await
        .expect("first operation should be approved");
    guard
        .execute("INSERT INTO jobs(id) VALUES (2)")
        .await
        .expect_err("second operation must request approval again");

    assert_eq!(approvals.load(Ordering::SeqCst), 2);
    assert_eq!(executes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn transaction_batch_requests_one_approval() {
    let executes = Arc::new(AtomicUsize::new(0));
    let approvals = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Prod,
            PolicyConfig::default(),
            Arc::new(CountingApprovalSink {
                calls: approvals.clone(),
            }),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    );
    let statements = vec![
        ("INSERT INTO jobs(id) VALUES (1)".into(), vec![]),
        ("INSERT INTO jobs(id) VALUES (2)".into(), vec![]),
    ];

    let rows = guard.execute_in_transaction(&statements).await.expect("approved batch");

    assert_eq!(rows, vec![1, 1]);
    assert_eq!(approvals.load(Ordering::SeqCst), 1);
    assert_eq!(executes.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn blast_radius_count_runs_through_the_controlled_query_path() {
    let queries = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        query_counting_connection(queries.clone()),
        context(
            Principal::human_gui(),
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    );

    guard
        .execute("UPDATE jobs SET status = 'done' WHERE id = 1")
        .await
        .expect("local write");

    assert_eq!(queries.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_side_effecting_predicate_is_never_run_by_the_blast_radius_count() {
    for sql in [
        "DELETE FROM jobs WHERE pg_terminate_backend(pid)",
        "UPDATE jobs SET status = 'done' WHERE nextval('job_seq') > 0",
    ] {
        let queries = Arc::new(AtomicUsize::new(0));
        let audit = Arc::new(SequenceAuditSink::new(vec![]));
        let guard = PolicyGuard::new(
            query_counting_connection(queries.clone()),
            context(
                Principal::human_gui(),
                Environment::Local,
                PolicyConfig::default(),
                Arc::new(AutoApproveSink),
                audit.clone(),
                Arc::new(AuditState::new()),
            ),
        );

        guard.execute(sql).await.expect("approved write");

        assert_eq!(queries.load(Ordering::SeqCst), 0, "{sql}");
        let events = audit.events.lock().expect("audit events");
        let Some(event) = events
            .iter()
            .find(|event| event.decision_rule.contains("human_dangerous_effect_approval"))
        else {
            panic!("the dangerous effect must remain in the approval decision: {events:?}");
        };
        assert!(
            event.decision_rule.contains("blast_radius_unknown"),
            "the joined verdict must retain the missing estimate reason: {event:?}"
        );
    }
}

#[tokio::test]
async fn an_expired_deadline_skips_the_blast_radius_count() {
    let queries = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        query_counting_connection(queries.clone()),
        context(
            Principal::human_gui(),
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    );
    let control = OperationControl::with_timeout(std::time::Duration::ZERO);

    let error = guard
        .execute_controlled("UPDATE jobs SET status = 'done' WHERE id = 1", &control)
        .await
        .expect_err("expired control must refuse the write");

    assert!(
        matches!(error, DriverError::TimedOut) || error.to_string().contains("timed out"),
        "{error}"
    );
    assert_eq!(queries.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn list_views_records_a_governed_read_outcome() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = PolicyGuard::new(
        connection(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );

    let views = guard.list_views().await.expect("list views");
    assert!(views.is_empty());

    let events = audit.events.lock().expect("event lock");
    let expected_hash = hex::encode(sha2::Sha256::digest(b"LIST VIEWS"));
    assert!(
        events
            .iter()
            .any(|event| event.decision_rule == "metadata_read" && event.sql_hash == expected_hash),
        "list_views must write a metadata audit record"
    );
}

#[tokio::test]
async fn list_databases_is_a_governed_read_with_its_own_audit_record() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = PolicyGuard::new(
        connection(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );

    let error = guard
        .list_databases()
        .await
        .expect_err("the stub driver lists no databases");
    assert!(matches!(error, DriverError::Unsupported(_)), "{error}");
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    let controlled_error = guard
        .list_databases_controlled(&control)
        .await
        .expect_err("the stub driver still lists no databases through the controlled path");
    assert!(
        matches!(controlled_error, DriverError::Unsupported(_)),
        "{controlled_error}"
    );

    let events = audit.events.lock().expect("event lock");
    let expected_hash = hex::encode(sha2::Sha256::digest(b"LIST DATABASES"));
    assert_eq!(
        events
            .iter()
            .filter(|event| event.decision_rule == "metadata_read" && event.sql_hash == expected_hash)
            .count(),
        2,
        "plain and controlled database listings must both be audited"
    );
    assert!(
        events
            .iter()
            .any(|event| event.decision_rule == "metadata_read" && event.sql_hash == expected_hash),
        "list_databases must write a metadata audit record"
    );
}

#[tokio::test]
async fn list_collations_is_a_governed_read_with_its_own_audit_record() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = PolicyGuard::new(
        connection(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );

    let error = guard
        .list_collations()
        .await
        .expect_err("the stub driver lists no collations");
    assert!(matches!(error, DriverError::Unsupported(_)), "{error}");
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    let controlled_error = guard
        .list_collations_controlled(&control)
        .await
        .expect_err("the stub driver still lists no collations through the controlled path");
    assert!(
        matches!(controlled_error, DriverError::Unsupported(_)),
        "{controlled_error}"
    );

    let events = audit.events.lock().expect("event lock");
    let expected_hash = hex::encode(sha2::Sha256::digest(b"LIST COLLATIONS"));
    assert_eq!(
        events
            .iter()
            .filter(|event| event.decision_rule == "metadata_read" && event.sql_hash == expected_hash)
            .count(),
        2,
        "plain and controlled collation listings must both be audited"
    );
    assert!(
        events
            .iter()
            .any(|event| event.decision_rule == "metadata_read" && event.sql_hash == expected_hash),
        "list_collations must write a metadata audit record"
    );
}

#[tokio::test]
async fn list_objects_reports_an_unsupported_kind_and_audits_the_read_with_its_target() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = PolicyGuard::new(
        connection(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );

    let error = guard
        .list_objects(bookie_core::CatalogObjectKind::Routine, Some("public"))
        .await
        .expect_err("the mock driver declares no catalog kinds");
    assert!(matches!(error, DriverError::Unsupported(_)), "{error:?}");

    let events = audit.events.lock().expect("event lock");
    let expected_hash = hex::encode(sha2::Sha256::digest(b"LIST OBJECTS"));
    assert!(
        events.iter().any(|event| event.decision_rule == "metadata_read"
            && event.sql_hash == expected_hash
            && event.targets == vec!["routine:public".to_string()]),
        "list_objects must write a metadata audit record naming kind and schema"
    );
}

#[tokio::test]
async fn list_objects_is_denied_when_the_agent_read_intent_cannot_be_recorded() {
    let guard = PolicyGuard::new(
        connection(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::Agent {
                token: "token".into(),
                client: None,
                model: None,
            },
            Environment::Local,
            allowed_agent_policy(),
            Arc::new(AutoApproveSink),
            Arc::new(SequenceAuditSink::new(vec![AuditRecordPhase::Intent])),
            Arc::new(AuditState::new()),
        ),
    );

    let error = guard
        .list_objects(bookie_core::CatalogObjectKind::Role, None)
        .await
        .expect_err("a failed read intent must deny the listing");

    assert!(!matches!(error, DriverError::Unsupported(_)), "{error:?}");
}

#[tokio::test]
async fn list_objects_audit_target_leaves_out_the_schema_for_unscoped_kinds() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = PolicyGuard::new(
        connection(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );

    let _ = guard
        .list_objects(bookie_core::CatalogObjectKind::Role, Some("public"))
        .await;

    let events = audit.events.lock().expect("event lock");
    assert!(events.iter().any(|event| event.targets == vec!["role".to_string()]));
}

fn tsql_spellings(driver_id: &str) -> &'static [&'static str] {
    if driver_id != "mssql" {
        return &[];
    }
    &[
        "BEGIN TRAN",
        "COMMIT TRAN",
        "ROLLBACK TRAN",
        "BEGIN TRAN UPDATE t SET a = 1 WHERE id = 1",
        "UPDATE t SET a = 1 WHERE id = 1 COMMIT TRAN",
    ]
}

#[tokio::test]
async fn transaction_control_on_the_shared_connection_is_denied_before_dispatch() {
    for driver_id in ["postgres", "mysql", "sqlite", "mssql"] {
        let common = ["BEGIN", "START TRANSACTION", "COMMIT", "ROLLBACK", "SELECT 1; BEGIN"];
        for statement in common.iter().chain(tsql_spellings(driver_id)).copied() {
            let executes = Arc::new(AtomicUsize::new(0));
            let queries = Arc::new(AtomicUsize::new(0));
            let audit = Arc::new(SequenceAuditSink::new(vec![]));
            let mut guard_context = context(
                Principal::human_gui(),
                Environment::Local,
                PolicyConfig::default(),
                Arc::new(AutoApproveSink),
                audit.clone(),
                Arc::new(AuditState::new()),
            );
            guard_context.driver_id = driver_id.into();
            let guard = PolicyGuard::new(
                Arc::new(QueryCountingConn {
                    queries: queries.clone(),
                }),
                guard_context,
            );

            let executed = guard.execute(statement).await;
            let queried = guard.query(statement).await;

            assert!(
                executed.is_err(),
                "{driver_id}: execute({statement}) reached a pooled connection"
            );
            assert!(
                queried.is_err(),
                "{driver_id}: query({statement}) reached a pooled connection"
            );
            assert_eq!(queries.load(Ordering::SeqCst), 0, "{driver_id}: {statement}");
            assert_eq!(executes.load(Ordering::SeqCst), 0);
            let events = audit.events.lock().expect("audit events");
            assert!(!events.is_empty(), "{driver_id}: {statement}: the denial is audited");
            assert!(
                events
                    .iter()
                    .all(|event| event.decision_rule.contains("transaction_control_needs_session")),
                "{driver_id}: {statement}: {:?}",
                events
                    .iter()
                    .map(|event| event.decision_rule.clone())
                    .collect::<Vec<_>>()
            );
        }
    }
}

fn transaction_guard(driver_id: &str, executes: Arc<AtomicUsize>, audit: Arc<SequenceAuditSink>) -> PolicyGuard {
    let mut guard_context = context(
        Principal::human_gui(),
        Environment::Local,
        PolicyConfig::default(),
        Arc::new(AutoApproveSink),
        audit,
        Arc::new(AuditState::new()),
    );
    guard_context.driver_id = driver_id.into();
    PolicyGuard::new(
        Arc::new(CountingConn {
            executes,
            commits: Arc::new(AtomicUsize::new(0)),
        }),
        guard_context,
    )
}

#[tokio::test]
async fn mysql_ddl_inside_a_rollback_able_transaction_is_denied_before_dispatch() {
    for statement in [
        "DROP TABLE t",
        "ALTER TABLE t ADD COLUMN c int",
        "CREATE TABLE u (id int)",
        "TRUNCATE TABLE t",
    ] {
        let executes = Arc::new(AtomicUsize::new(0));
        let audit = Arc::new(SequenceAuditSink::new(vec![]));
        let guard = transaction_guard("mysql", executes.clone(), audit.clone());

        let mut transaction = guard.begin().await.expect("begin");
        let executed = transaction.execute(statement).await;

        assert!(
            matches!(executed, Err(DriverError::PolicyDenied(_))),
            "{statement}: {executed:?}"
        );
        assert_eq!(executes.load(Ordering::SeqCst), 0, "{statement} reached the driver");
        let events = audit.events.lock().expect("audit events");
        assert!(
            events.iter().any(|event| {
                event.phase == AuditRecordPhase::Outcome && event.terminal_status == AuditTerminalStatus::Denied
            }),
            "{statement}: the denial is audited"
        );
    }
}

#[tokio::test]
async fn mysql_non_committing_statements_run_inside_an_interactive_transaction() {
    for statement in [
        "SELECT * FROM items WHERE id = 1 FOR UPDATE",
        "SET @value = 1",
        "SET @autocommit = 1",
        "SET @first = 1, @second = 2",
        "SAVEPOINT saved_state",
        "ROLLBACK TO SAVEPOINT saved_state",
        "RELEASE SAVEPOINT saved_state",
        "SHOW TABLES",
        "SHOW WARNINGS",
        "SHOW STATUS",
        "SHOW VARIABLES",
        "SHOW CREATE TABLE items",
        "SHOW COLUMNS FROM items",
        "SHOW CATALOGS",
        "SHOW DATABASES",
        "SHOW PROCESSLIST",
        "SHOW SCHEMAS",
        "SHOW CHARACTER SET",
        "SHOW VIEWS",
        "SHOW COLLATION",
    ] {
        let executes = Arc::new(AtomicUsize::new(0));
        let guard = transaction_guard("mysql", executes.clone(), Arc::new(SequenceAuditSink::new(vec![])));

        let mut transaction = guard.begin().await.expect("begin");
        transaction.execute(statement).await.expect(statement);

        assert_eq!(executes.load(Ordering::SeqCst), 1, "{statement} reached the driver");
    }
}

#[tokio::test]
async fn mysql_transaction_statements_that_change_transaction_ownership_are_denied() {
    for statement in [
        "BEGIN",
        "COMMIT",
        "ROLLBACK",
        "LOCK TABLES items WRITE",
        "CREATE USER 'other'@'localhost'",
        "SET autocommit = 0",
        "SET autocommit = 1",
        "SET @@session.autocommit = 1",
        "SET @value = 1; DROP TABLE items",
        "SELECT * INTO OUTFILE '/tmp/bookie-out.csv' FROM items",
    ] {
        let executes = Arc::new(AtomicUsize::new(0));
        let guard = transaction_guard("mysql", executes.clone(), Arc::new(SequenceAuditSink::new(vec![])));

        let mut transaction = guard.begin().await.expect("begin");
        let result = transaction.execute(statement).await;

        assert!(
            matches!(result, Err(DriverError::PolicyDenied(_))),
            "{statement}: {result:?}"
        );
        assert_eq!(executes.load(Ordering::SeqCst), 0, "{statement} reached the driver");
    }
}

#[tokio::test]
async fn transactional_ddl_and_mysql_dml_still_run_inside_a_transaction() {
    for (driver_id, statement) in [
        ("postgres", "DROP TABLE t"),
        ("sqlite", "CREATE TABLE u (id int)"),
        ("mysql", "UPDATE t SET a = 1 WHERE id = 1"),
    ] {
        let executes = Arc::new(AtomicUsize::new(0));
        let guard = transaction_guard(driver_id, executes.clone(), Arc::new(SequenceAuditSink::new(vec![])));

        let mut transaction = guard.begin().await.expect("begin");
        transaction.execute(statement).await.expect(statement);

        assert_eq!(executes.load(Ordering::SeqCst), 1, "{driver_id}: {statement}");
    }
}

#[tokio::test]
async fn an_approved_controlled_write_never_cancels_the_callers_token() {
    let executes = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Prod,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    );
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));

    guard
        .execute_controlled("UPDATE jobs SET status = 'done' WHERE id = 1", &control)
        .await
        .expect("the approved write runs");
    guard
        .execute_controlled("UPDATE jobs SET status = 'new' WHERE id = 2", &control)
        .await
        .expect("the next statement on the same control runs");

    assert_eq!(executes.load(Ordering::SeqCst), 2);
    assert!(!control.cancellation_token().is_cancelled());
}

struct StalledVersionConn(Arc<Notify>);

#[async_trait]
impl Connection for StalledVersionConn {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(vec![])
    }

    async fn server_version(&self) -> Result<Option<String>, DriverError> {
        self.0.notify_one();
        std::future::pending().await
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

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

fn stalled_version_guard(audit: Arc<SequenceAuditSink>, state: Arc<AuditState>) -> (PolicyGuard, Arc<Notify>) {
    let dispatched = Arc::new(Notify::new());
    let guard = PolicyGuard::new(
        Arc::new(StalledVersionConn(dispatched.clone())),
        context(
            Principal::human_gui(),
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            audit,
            state,
        ),
    );
    (guard, dispatched)
}

fn last_terminal_status(audit: &SequenceAuditSink) -> Option<AuditTerminalStatus> {
    audit
        .events
        .lock()
        .expect("audit events")
        .iter()
        .rev()
        .find(|event| event.phase == AuditRecordPhase::Outcome)
        .map(|event| event.terminal_status)
}

#[tokio::test]
async fn cancelling_a_version_request_records_a_terminal_unknown_outcome_and_keeps_writes_enabled() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let state = Arc::new(AuditState::new());
    let (guard, dispatched) = stalled_version_guard(audit.clone(), state.clone());
    let token = tokio_util::sync::CancellationToken::new();
    let control = OperationControl::new(
        token.clone(),
        Some(tokio::time::Instant::now() + Duration::from_secs(30)),
    );
    tokio::spawn(async move {
        dispatched.notified().await;
        token.cancel();
    });

    let result = guard.server_version_controlled(&control).await;

    assert!(
        matches!(&result, Err(DriverError::OperationOutcomeUnknown { source }) if matches!(**source, DriverError::Cancelled)),
        "{result:?}"
    );
    assert_eq!(last_terminal_status(&audit), Some(AuditTerminalStatus::Unknown));
    assert!(
        !state.governed_writes_disabled(),
        "an interrupted read must not disable writes"
    );
}

#[tokio::test]
async fn a_version_request_past_its_deadline_records_a_terminal_outcome() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let (guard, _dispatched) = stalled_version_guard(audit.clone(), Arc::new(AuditState::new()));
    let control = OperationControl::new(
        tokio_util::sync::CancellationToken::new(),
        Some(tokio::time::Instant::now() + Duration::from_millis(100)),
    );

    let result = tokio::time::timeout(Duration::from_secs(5), guard.server_version_controlled(&control))
        .await
        .expect("the deadline must end the version request");

    assert!(
        matches!(&result, Err(DriverError::OperationOutcomeUnknown { source }) if matches!(**source, DriverError::TimedOut)),
        "{result:?}"
    );
    assert_eq!(last_terminal_status(&audit), Some(AuditTerminalStatus::Unknown));
}

#[tokio::test]
async fn an_already_cancelled_control_refuses_the_version_request_before_it_is_audited_or_sent() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let (guard, _dispatched) = stalled_version_guard(audit.clone(), Arc::new(AuditState::new()));
    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();
    let control = OperationControl::new(token, None);

    let result = guard.server_version_controlled(&control).await;

    assert!(matches!(result, Err(DriverError::Cancelled)), "{result:?}");
    assert!(audit.events.lock().expect("audit events").is_empty());
}
