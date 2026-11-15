use super::*;

struct SilentApprovalSink;

#[async_trait]
impl ApprovalSink for SilentApprovalSink {
    async fn request(&self, _request: ApprovalRequest) -> ApprovalOutcome {
        std::future::pending().await
    }
}

struct SlowApprovalSink {
    delay: std::time::Duration,
}

#[async_trait]
impl ApprovalSink for SlowApprovalSink {
    async fn request(&self, _request: ApprovalRequest) -> ApprovalOutcome {
        tokio::time::sleep(self.delay).await;
        ApprovalOutcome::AllowOnce
    }
}

fn approving_policy() -> PolicyConfig {
    let mut policy = PolicyConfig::default();
    policy
        .environments
        .entry("local".into())
        .or_default()
        .human_approve_writes = Some(true);
    policy
}

#[tokio::test(start_paused = true)]
async fn an_approval_nobody_answers_times_out_and_the_write_never_runs() {
    let executes = Arc::new(AtomicUsize::new(0));
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Local,
            approving_policy(),
            Arc::new(SilentApprovalSink),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );
    let started = tokio::time::Instant::now();

    let error = guard
        .execute("DELETE FROM items WHERE id = 1")
        .await
        .expect_err("an unanswered approval must not allow the write");

    assert!(matches!(error, DriverError::TimedOut), "{error:?}");
    assert_eq!(started.elapsed(), APPROVAL_WAIT_LIMIT);
    assert_eq!(executes.load(Ordering::SeqCst), 0);
    let events = audit.events.lock().expect("audit events");
    let outcome = events.last().expect("terminal outcome");
    assert_eq!(outcome.terminal_status, AuditTerminalStatus::TimedOut);
    assert_eq!(outcome.approval_outcome, AuditApprovalOutcome::TimedOut);
}

#[tokio::test(start_paused = true)]
async fn an_approval_answered_before_the_limit_still_runs_the_write() {
    let executes = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        context(
            Principal::human_gui(),
            Environment::Local,
            approving_policy(),
            Arc::new(SlowApprovalSink {
                delay: APPROVAL_WAIT_LIMIT - std::time::Duration::from_secs(1),
            }),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    );

    guard
        .execute("DELETE FROM items WHERE id = 1")
        .await
        .expect("an approval inside the limit allows the write");

    assert_eq!(executes.load(Ordering::SeqCst), 1);
}

struct CountingApproveSink {
    prompts: Arc<AtomicUsize>,
}

#[async_trait]
impl ApprovalSink for CountingApproveSink {
    async fn request(&self, _request: ApprovalRequest) -> ApprovalOutcome {
        self.prompts.fetch_add(1, Ordering::SeqCst);
        ApprovalOutcome::AllowOnce
    }
}

fn counting_rig(principal: Principal) -> (PolicyGuard, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let executes = Arc::new(AtomicUsize::new(0));
    let prompts = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        context(
            principal,
            Environment::Local,
            approving_policy(),
            Arc::new(CountingApproveSink {
                prompts: prompts.clone(),
            }),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    );
    (guard, prompts, executes)
}

fn agent() -> Principal {
    Principal::Agent {
        token: "token".into(),
        client: None,
        model: None,
    }
}

#[tokio::test]
async fn an_agent_write_the_approval_dialog_cannot_show_in_full_is_denied_without_asking() {
    let long = format!(
        "UPDATE items SET memo = '{}', balance = 0",
        "x".repeat(crate::approval::MAX_APPROVAL_SQL_CHARS)
    );
    for sql in [
        long.as_str(),
        "UPDATE items SET memo = '\u{202E}reversed' WHERE id = 1",
        "UPDATE items SET memo = 'zero\u{200B}width' WHERE id = 1",
    ] {
        let (guard, prompts, executes) = counting_rig(agent());

        let error = guard
            .execute(sql)
            .await
            .expect_err("an unreviewable statement is refused");

        assert!(matches!(error, DriverError::PolicyDenied(_)), "{error:?}");
        assert_eq!(
            prompts.load(Ordering::SeqCst),
            0,
            "no one is asked to approve what they cannot read"
        );
        assert_eq!(executes.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn an_ordinary_agent_write_and_a_long_human_write_are_still_asked_and_run() {
    let long = format!(
        "UPDATE items SET memo = '{}' WHERE id = 1",
        "x".repeat(crate::approval::MAX_APPROVAL_SQL_CHARS)
    );
    let (guard, prompts, executes) = counting_rig(agent());
    guard
        .execute("UPDATE items SET memo = 'line one\nline two' WHERE id = 1")
        .await
        .expect("a readable agent statement is approved normally");
    assert_eq!(
        (prompts.load(Ordering::SeqCst), executes.load(Ordering::SeqCst)),
        (1, 1)
    );

    let (guard, prompts, executes) = counting_rig(Principal::human_gui());
    guard
        .execute(&long)
        .await
        .expect("a person sees the dialog and can scroll it");
    assert_eq!(
        (prompts.load(Ordering::SeqCst), executes.load(Ordering::SeqCst)),
        (1, 1)
    );
}

#[tokio::test(start_paused = true)]
async fn an_approval_slower_than_the_statement_timeout_does_not_use_up_the_statements_time() {
    let executes = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        context(
            agent(),
            Environment::Local,
            approving_policy(),
            Arc::new(SlowApprovalSink {
                delay: std::time::Duration::from_secs(31),
            }),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    );
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));

    guard
        .execute_controlled("DELETE FROM items WHERE id = 1", &control)
        .await
        .expect("a person reading for 31 seconds must not make the write time out");

    assert_eq!(executes.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn an_unanswered_approval_stops_at_the_approval_limit_even_with_a_shorter_statement_timeout() {
    let executes = Arc::new(AtomicUsize::new(0));
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        context(
            agent(),
            Environment::Local,
            approving_policy(),
            Arc::new(SilentApprovalSink),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    );
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    let started = tokio::time::Instant::now();

    let error = guard
        .execute_controlled("DELETE FROM items WHERE id = 1", &control)
        .await
        .expect_err("nobody answers");

    assert!(matches!(error, DriverError::TimedOut), "{error:?}");
    assert_eq!(started.elapsed(), APPROVAL_WAIT_LIMIT);
    assert_eq!(executes.load(Ordering::SeqCst), 0);
}
