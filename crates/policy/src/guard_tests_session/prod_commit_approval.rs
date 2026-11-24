use super::*;

struct CommitAnswer {
    commit: ApprovalOutcome,
    requests: Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl ApprovalSink for CommitAnswer {
    async fn request(&self, request: ApprovalRequest) -> ApprovalOutcome {
        let is_commit = request.rule == "prod_commit_requires_approval";
        self.requests
            .lock()
            .expect("request lock")
            .push((request.rule, request.reason));
        if is_commit {
            self.commit
        } else {
            ApprovalOutcome::AllowOnce
        }
    }
}

fn prod_harness(commit: ApprovalOutcome) -> (Harness, Arc<CommitAnswer>) {
    let answer = Arc::new(CommitAnswer {
        commit,
        requests: Mutex::new(Vec::new()),
    });
    let script = Arc::new(SessionScript::default());
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let audit_state = Arc::new(AuditState::new());
    let mut ctx = context(
        Principal::human_gui(),
        Environment::Prod,
        PolicyConfig::default(),
        answer.clone(),
        audit.clone(),
        audit_state.clone(),
    );
    ctx.driver_id = "postgres".into();
    let inner: Arc<dyn Connection> = Arc::new(SessionConn { script: script.clone() });
    let harness = Harness {
        guard: PolicyGuard::new(inner, ctx),
        script,
        audit,
        audit_state,
    };
    (harness, answer)
}

fn commit_requests(answer: &CommitAnswer) -> Vec<String> {
    answer
        .requests
        .lock()
        .expect("request lock")
        .iter()
        .filter(|(rule, _)| rule == "prod_commit_requires_approval")
        .map(|(_, reason)| reason.clone())
        .collect()
}

fn sent(harness: &Harness) -> Vec<String> {
    harness.script.sent.lock().expect("sent lock").clone()
}

#[tokio::test]
async fn a_denied_production_commit_is_not_sent_and_the_transaction_stays_open() {
    let (h, answer) = prod_harness(ApprovalOutcome::Deny);
    let mut session = h.guard.open_session().await.expect("open session");
    run(&mut session, "BEGIN").await.expect("begin");
    run(&mut session, "UPDATE items SET v = 1 WHERE id = 1")
        .await
        .expect("approved write");
    run(&mut session, "UPDATE items SET v = 2 WHERE id = 2")
        .await
        .expect("approved write");

    let error = run(&mut session, "COMMIT")
        .await
        .expect_err("a denied commit must not run");

    assert!(matches!(error, DriverError::PolicyDenied(_)), "{error:?}");
    assert!(!sent(&h).iter().any(|sql| sql == "COMMIT"), "{:?}", sent(&h));
    assert!(session.transaction_open(), "the person can still roll back");
    assert_eq!(
        commit_requests(&answer),
        ["Commit 2 governed writes on a production connection"]
    );
    run(&mut session, "ROLLBACK")
        .await
        .expect("roll back after the refusal");
    assert!(!session.transaction_open());
}

#[tokio::test]
async fn an_approved_production_commit_runs_once_the_person_agrees() {
    let (h, answer) = prod_harness(ApprovalOutcome::AllowOnce);
    let mut session = h.guard.open_session().await.expect("open session");
    run(&mut session, "BEGIN").await.expect("begin");
    run(&mut session, "UPDATE items SET v = 1 WHERE id = 1")
        .await
        .expect("approved write");

    run(&mut session, "COMMIT").await.expect("approved commit");

    assert!(sent(&h).iter().any(|sql| sql == "COMMIT"));
    assert_eq!(
        commit_requests(&answer),
        ["Commit 1 governed write on a production connection"]
    );
    let commits = outcomes_of(&h.audit, AuditOperationClass::TransactionCommit);
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].transaction_outcome, AuditTransactionOutcome::Committed);
}

#[tokio::test]
async fn a_production_commit_without_writes_and_a_local_commit_do_not_ask() {
    let (h, answer) = prod_harness(ApprovalOutcome::Deny);
    let mut session = h.guard.open_session().await.expect("open session");
    run(&mut session, "BEGIN").await.expect("begin");
    run(&mut session, "SELECT 1").await.expect("read");
    run(&mut session, "COMMIT")
        .await
        .expect("a read-only transaction commits without a prompt");
    assert!(commit_requests(&answer).is_empty());

    let local = harness(SessionScript::default(), false);
    let mut session = local.guard.open_session().await.expect("open session");
    for sql in ["BEGIN", "INSERT INTO items VALUES (1)", "COMMIT"] {
        run(&mut session, sql).await.expect(sql);
    }
}

struct SlowCommitApproval;

#[async_trait]
impl ApprovalSink for SlowCommitApproval {
    async fn request(&self, request: ApprovalRequest) -> ApprovalOutcome {
        if request.rule == "prod_commit_requires_approval" {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
        ApprovalOutcome::AllowOnce
    }
}

#[tokio::test(start_paused = true)]
async fn a_production_commit_approved_after_its_timeout_still_commits() {
    let (mut h, _) = prod_harness(ApprovalOutcome::AllowOnce);
    let mut ctx = context(
        Principal::human_gui(),
        Environment::Prod,
        PolicyConfig::default(),
        Arc::new(SlowCommitApproval),
        h.audit.clone(),
        h.audit_state.clone(),
    );
    ctx.driver_id = "postgres".into();
    h.script = Arc::new(SessionScript {
        honor_deadline: true,
        ..SessionScript::default()
    });
    let inner: Arc<dyn Connection> = Arc::new(SessionConn {
        script: h.script.clone(),
    });
    h.guard = PolicyGuard::new(inner, ctx);
    let mut session = h.guard.open_session().await.expect("open session");
    run(&mut session, "BEGIN").await.expect("begin");
    run(&mut session, "UPDATE items SET v = 1 WHERE id = 1")
        .await
        .expect("write");

    session
        .query_params_controlled("COMMIT", &[], &OperationControl::with_timeout(Duration::from_secs(30)))
        .await
        .expect("the approval wait does not count against the commit timeout");

    assert!(sent(&h).iter().any(|sql| sql == "COMMIT"), "{:?}", sent(&h));
    assert!(!session.transaction_open());
}
