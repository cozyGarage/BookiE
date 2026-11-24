#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use async_trait::async_trait;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, Environment, OperationControl, Value};
use bookie_policy::{
    ApprovalOutcome, ApprovalRequest, ApprovalSink, AuditError, AuditEvent, AuditSink, GuardContext, PolicyConfig,
    PolicyGuard, Principal,
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Audit(Mutex<Vec<AuditEvent>>);
#[async_trait]
impl AuditSink for Audit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.0.lock().unwrap().push(event);
        Ok(())
    }
}
struct DenyCommit;
#[async_trait]
impl ApprovalSink for DenyCommit {
    async fn request(&self, request: ApprovalRequest) -> ApprovalOutcome {
        if request.rule == "prod_commit_requires_approval" {
            ApprovalOutcome::Deny
        } else {
            ApprovalOutcome::AllowOnce
        }
    }
}

#[tokio::test]
async fn denied_guarded_commit_remains_open_until_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    std::fs::File::create(&path).unwrap();
    let raw: Arc<dyn Connection> = Arc::from(
        drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: path.to_string_lossy().into_owned(),
                ..Default::default()
            })
            .await
            .unwrap(),
    );
    raw.execute("CREATE TABLE item (id INTEGER PRIMARY KEY, value TEXT)")
        .await
        .unwrap();
    raw.execute("INSERT INTO item VALUES (1, 'before')").await.unwrap();
    let audit = Arc::new(Audit::default());
    let guard = PolicyGuard::new(
        raw.clone(),
        GuardContext {
            connection_id: uuid::Uuid::new_v4(),
            connection_name: "test".into(),
            driver_id: "sqlite".into(),
            environment: Environment::Prod,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(DenyCommit),
            audit: audit.clone(),
            audit_state: Arc::new(bookie_policy::AuditState::new()),
        },
    );
    let mut session = guard.open_session().await.unwrap();
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(10));
    session.query_params_controlled("BEGIN", &[], &control).await.unwrap();
    session
        .query_params_controlled("UPDATE item SET value = 'after' WHERE id = 1", &[], &control)
        .await
        .unwrap();
    let shared = Arc::new(tokio::sync::Mutex::new(Some(
        bookie_app_services::interactive_session::SessionHandle::plain(session),
    )));
    assert!(matches!(
        bookie_app_services::interactive_session::commit_session(&shared).await,
        Err(bookie_core::DriverError::PolicyDenied(_))
    ));
    bookie_app_services::interactive_session::rollback_and_close(&shared)
        .await
        .unwrap();
    let result = raw.query("SELECT value FROM item WHERE id = 1").await.unwrap();
    assert_eq!(result.rows[0][0], Value::Text("before".into()));
    assert!(
        audit
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.decision_rule == "prod_commit_requires_approval"
                && event.terminal_status == bookie_policy::AuditTerminalStatus::Denied)
    );
}
