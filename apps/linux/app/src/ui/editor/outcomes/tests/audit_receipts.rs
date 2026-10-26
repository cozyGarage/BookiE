use super::*;

struct ImmediateApproval;

#[async_trait::async_trait]
impl tablepro_policy::ApprovalSink for ImmediateApproval {
    async fn request(&self, _request: tablepro_policy::ApprovalRequest) -> tablepro_policy::ApprovalOutcome {
        tablepro_policy::ApprovalOutcome::AllowOnce
    }
}

async fn audited_sqlite_guard() -> std::sync::Arc<tablepro_policy::PolicyGuard> {
    audited_guard(sqlite_connection().await).await
}

async fn audited_guard(
    connection: std::sync::Arc<dyn tablepro_core::Connection>,
) -> std::sync::Arc<tablepro_policy::PolicyGuard> {
    std::sync::Arc::new(tablepro_policy::PolicyGuard::new(
        connection,
        tablepro_policy::GuardContext {
            connection_id: uuid::Uuid::new_v4(),
            connection_name: "audit receipt test".into(),
            driver_id: "sqlite".into(),
            environment: tablepro_core::Environment::Local,
            read_only: false,
            principal: tablepro_policy::Principal::human_gui(),
            policy: std::sync::Arc::new(tablepro_policy::PolicyConfig::default()),
            approval: std::sync::Arc::new(ImmediateApproval),
            audit: std::sync::Arc::new(super::QuietAudit),
            audit_state: std::sync::Arc::new(tablepro_policy::AuditState::new()),
        },
    ))
}

fn failure_receipts(result: ScriptRunResult) -> Vec<uuid::Uuid> {
    let ScriptRunResult::Completed(outcomes) = result else {
        panic!("expected a completed statement outcome")
    };
    let Some(StatementOutcomeKind::Error(failure)) = outcomes.first().map(|outcome| &outcome.kind) else {
        panic!("expected a statement failure")
    };
    failure.audit_operation_ids().to_vec()
}

async fn fail_one_statement(target: StatementTarget, sql: &str) -> Vec<uuid::Uuid> {
    failure_receipts(
        run_statements(
            target,
            vec![sql.into()],
            "sqlite",
            &Default::default(),
            &crate::services::operation_control::bounded(0),
            BatchErrorPolicy::StopScript,
            |_| {},
        )
        .await,
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_sql_failures_keep_their_own_audit_operation_ids() {
    let target = StatementTarget::GuardedPool(audited_sqlite_guard().await);
    let (first, second) = tokio::join!(
        fail_one_statement(target.clone(), "SELECT missing FROM first_table"),
        fail_one_statement(target, "SELECT missing FROM second_table"),
    );

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_ne!(first[0], second[0]);
}

#[tokio::test]
async fn a_manual_session_failure_keeps_its_audit_operation_id() {
    let directory = tempfile::tempdir().unwrap();
    let connection = drivers_sqlite::SqliteDriver
        .connect(tablepro_core::ConnectOptions {
            database: directory.path().join("session.sqlite").to_string_lossy().into_owned(),
            ..Default::default()
        })
        .await
        .expect("file-backed sqlite connection");
    let session = audited_guard(std::sync::Arc::from(connection))
        .await
        .open_session_with_audit()
        .await
        .expect("audited session");
    let target = StatementTarget::Session {
        id: uuid::Uuid::new_v4(),
        shared: std::sync::Arc::new(tokio::sync::Mutex::new(Some(
            super::super::super::session_mode::SessionHandle::Audited(session),
        ))),
    };

    let receipts = fail_one_statement(target, "SELECT missing FROM session_table").await;

    assert_eq!(receipts.len(), 1);
}
