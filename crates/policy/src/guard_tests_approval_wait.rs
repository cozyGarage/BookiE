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
