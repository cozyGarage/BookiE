use super::*;

async fn approvals_for(count_in_session: i64) -> usize {
    let approvals = Arc::new(AtomicUsize::new(0));
    let script = Arc::new(SessionScript {
        blast_count: Some(count_in_session),
        ..SessionScript::default()
    });
    let mut ctx = context(
        Principal::human_gui(),
        Environment::Staging,
        PolicyConfig::default(),
        Arc::new(CountingApprovalSink {
            calls: approvals.clone(),
        }),
        Arc::new(SequenceAuditSink::new(vec![])),
        Arc::new(AuditState::new()),
    );
    ctx.driver_id = "postgres".into();
    let guard = PolicyGuard::new(Arc::new(SessionConn { script }), ctx);
    let mut session = guard.open_session().await.expect("open session");
    run(&mut session, "BEGIN").await.expect("begin");
    run(&mut session, "UPDATE items SET flag = 1 WHERE flag IS NULL")
        .await
        .expect("an approved write runs");
    approvals.load(Ordering::SeqCst)
}

#[tokio::test]
async fn the_blast_radius_is_counted_inside_the_session_that_will_run_the_write() {
    assert_eq!(
        approvals_for(1).await,
        0,
        "a small count seen by the session needs no approval"
    );
    assert_eq!(
        approvals_for(6_000).await,
        1,
        "a count above the limit seen by the session asks for approval"
    );
}
