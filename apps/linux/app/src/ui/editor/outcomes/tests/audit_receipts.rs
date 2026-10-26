use super::*;

struct FailingCursorConnection;

#[derive(Default)]
struct CapturedAuditEvents(std::sync::Mutex<Vec<tablepro_policy::AuditEvent>>);

#[async_trait::async_trait]
impl tablepro_policy::AuditSink for CapturedAuditEvents {
    async fn record(&self, event: tablepro_policy::AuditEvent) -> Result<(), tablepro_policy::AuditError> {
        self.0.lock().unwrap().push(event);
        Ok(())
    }
}

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

fn audited_cursor_target() -> (StatementTarget, std::sync::Arc<CapturedAuditEvents>) {
    let connection: std::sync::Arc<dyn tablepro_core::Connection> = std::sync::Arc::new(FailingCursorConnection);
    let audit = std::sync::Arc::new(CapturedAuditEvents::default());
    let guard = tablepro_policy::PolicyGuard::new(
        connection,
        tablepro_policy::GuardContext {
            connection_id: uuid::Uuid::new_v4(),
            connection_name: "cursor receipt test".into(),
            driver_id: "postgres".into(),
            environment: tablepro_core::Environment::Local,
            read_only: false,
            principal: tablepro_policy::Principal::human_gui(),
            policy: std::sync::Arc::new(tablepro_policy::PolicyConfig::default()),
            approval: std::sync::Arc::new(ImmediateApproval),
            audit: audit.clone(),
            audit_state: std::sync::Arc::new(tablepro_policy::AuditState::new()),
        },
    );
    (StatementTarget::GuardedPool(std::sync::Arc::new(guard)), audit)
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

#[tokio::test]
async fn a_streamed_cursor_failure_keeps_its_recorded_audit_operation_ids() {
    let (target, audit) = audited_cursor_target();
    let result = run_statements_with_stream(
        StatementRun {
            target,
            statements: vec!["SELECT id FROM items".into()],
            driver_id: "postgres",
            parameter_values: &Default::default(),
            control: &crate::services::operation_control::bounded(30),
            error_policy: BatchErrorPolicy::StopScript,
            settings_check: None,
            succeeded: |_: &str| {},
        },
        |_| async { true },
    )
    .await;
    let ScriptRunResult::Streamed(StreamTerminal::Failed(failure)) = result else {
        panic!("the cursor fetch failure should remain a failed stream")
    };

    let failed_operation_id = audit
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|event| event.terminal_status == tablepro_policy::AuditTerminalStatus::Failed)
        .map(|event| event.operation_id)
        .expect("failed cursor fetch has a terminal audit record");

    assert!(failure.audit_operation_ids().contains(&failed_operation_id));
}

#[async_trait::async_trait]
impl tablepro_core::Connection for FailingCursorConnection {
    async fn list_tables(&self) -> Result<Vec<tablepro_core::TableInfo>, tablepro_core::DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_columns(
        &self,
        _schema: Option<&str>,
        _table: &str,
    ) -> Result<Vec<tablepro_core::ColumnInfo>, tablepro_core::DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_rows(
        &self,
        _schema: Option<&str>,
        _table: &str,
        _offset: u64,
        _limit: u64,
    ) -> Result<tablepro_core::QueryResult, tablepro_core::DriverError> {
        Ok(tablepro_core::QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            truncated: false,
        })
    }

    async fn query(&self, _sql: &str) -> Result<tablepro_core::QueryResult, tablepro_core::DriverError> {
        Ok(tablepro_core::QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            truncated: false,
        })
    }

    async fn execute(&self, _sql: &str) -> Result<tablepro_core::ExecResult, tablepro_core::DriverError> {
        Ok(tablepro_core::ExecResult { rows_affected: 0 })
    }

    async fn execute_params(
        &self,
        _sql: &str,
        _params: &[tablepro_core::Value],
    ) -> Result<tablepro_core::ExecResult, tablepro_core::DriverError> {
        self.execute("").await
    }

    async fn execute_in_transaction(
        &self,
        statements: &[(String, Vec<tablepro_core::Value>)],
    ) -> Result<Vec<u64>, tablepro_core::DriverError> {
        Ok(vec![0; statements.len()])
    }

    async fn ping(&self) -> Result<(), tablepro_core::DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), tablepro_core::DriverError> {
        Ok(())
    }

    async fn open_session(&self) -> Result<Box<dyn tablepro_core::Session>, tablepro_core::DriverError> {
        Ok(Box::new(super::CursorSession {
            pages: std::collections::VecDeque::from([Err(tablepro_core::DriverError::Query {
                message: "cursor fetch failed".into(),
                sqlstate: Some("XX000".into()),
                position: None,
            })]),
            counters: std::sync::Arc::new(super::CursorCounters::default()),
            transaction_open: false,
            cursor_open: false,
        }))
    }
}
