#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bookie_core::{
    ColumnInfo, Connection, DriverError, Environment, ExecResult, MessageSeverity, OperationControl, QueryResult,
    QueryResultBatch, ServerMessage, ServerMessages, Session, TableInfo, Value,
};
use bookie_policy::{
    AuditError, AuditEvent, AuditSink, AuditState, AutoApproveSink, DenyApprovalSink, GuardContext, PolicyConfig,
    PolicyGuard, Principal,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const MESSAGE_TEXT: &str = "card 4111111111111111 was seen";

#[derive(Default)]
struct RecordingAudit {
    events: Mutex<Vec<AuditEvent>>,
}

#[async_trait]
impl AuditSink for RecordingAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

struct NoisySession;

fn noisy_messages() -> ServerMessages {
    let mut messages = ServerMessages::default();
    messages.push(ServerMessage::new(
        MessageSeverity::Notice,
        Some("00000".into()),
        MESSAGE_TEXT,
    ));
    messages
}

#[async_trait]
impl Session for NoisySession {
    async fn query_params_controlled(
        &mut self,
        _sql: &str,
        _params: &[Value],
        _control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        Ok(empty_result())
    }

    async fn query_result_sets_params_controlled(
        &mut self,
        _sql: &str,
        _params: &[Value],
        _control: &OperationControl,
    ) -> Result<QueryResultBatch, DriverError> {
        Ok(QueryResultBatch::single(empty_result()).with_messages(noisy_messages()))
    }

    fn is_usable(&self) -> bool {
        true
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct NoisyConnection;

#[async_trait]
impl Connection for NoisyConnection {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_columns(&self, _schema: Option<&str>, _table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_rows(
        &self,
        _schema: Option<&str>,
        _table: &str,
        _offset: u64,
        _limit: u64,
    ) -> Result<QueryResult, DriverError> {
        Ok(empty_result())
    }

    async fn query(&self, _sql: &str) -> Result<QueryResult, DriverError> {
        Ok(empty_result())
    }

    async fn query_result_sets_controlled(
        &self,
        _sql: &str,
        _params: &[Value],
        _control: &OperationControl,
    ) -> Result<QueryResultBatch, DriverError> {
        Ok(QueryResultBatch::single(empty_result()).with_messages(noisy_messages()))
    }

    async fn open_session(&self) -> Result<Box<dyn Session>, DriverError> {
        Ok(Box::new(NoisySession))
    }

    async fn execute(&self, _sql: &str) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 0 })
    }

    async fn execute_params(&self, _sql: &str, _params: &[Value]) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 0 })
    }

    async fn execute_in_transaction(&self, _statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Ok(Vec::new())
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

fn empty_result() -> QueryResult {
    QueryResult {
        columns: Vec::new(),
        rows: Vec::new().into(),
        truncated: false,
    }
}

fn guard_for(principal: Principal, audit: Arc<RecordingAudit>) -> PolicyGuard {
    let approval: Arc<dyn bookie_policy::ApprovalSink> = if principal.is_agent() {
        Arc::new(AutoApproveSink)
    } else {
        Arc::new(DenyApprovalSink)
    };
    PolicyGuard::new(
        Arc::new(NoisyConnection),
        GuardContext {
            connection_id: Uuid::new_v4(),
            connection_name: "noisy".into(),
            driver_id: "postgres".into(),
            environment: Environment::Local,
            read_only: false,
            principal,
            policy: Arc::new(PolicyConfig::default()),
            approval,
            audit,
            audit_state: Arc::new(AuditState::new()),
        },
    )
}

fn human() -> Principal {
    Principal::Human {
        session: "test-session".into(),
    }
}

fn agent() -> Principal {
    Principal::Agent {
        token: "test-token".into(),
        client: None,
        model: None,
    }
}

fn control() -> OperationControl {
    OperationControl::new(CancellationToken::new(), None)
}

#[tokio::test]
async fn a_human_guard_passes_server_messages_through_unchanged() {
    let audit = Arc::new(RecordingAudit::default());
    let batch = guard_for(human(), audit)
        .query_result_sets_controlled("SELECT 1", &[], &control())
        .await
        .unwrap();
    let messages: Vec<_> = batch.messages.iter().collect();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, MESSAGE_TEXT);
    assert_eq!(messages[0].severity, MessageSeverity::Notice);
}

#[tokio::test]
async fn an_agent_guard_withholds_server_messages_because_they_can_echo_data() {
    let audit = Arc::new(RecordingAudit::default());
    let batch = guard_for(agent(), audit)
        .query_result_sets_controlled("SELECT 1", &[], &control())
        .await
        .unwrap();
    assert!(batch.messages.is_empty());
}

#[tokio::test]
async fn the_audit_journal_never_receives_message_text() {
    for principal in [human(), agent()] {
        let audit = Arc::new(RecordingAudit::default());
        guard_for(principal, audit.clone())
            .query_result_sets_controlled("SELECT 1", &[], &control())
            .await
            .unwrap();
        let events = audit.events.lock().unwrap();
        assert!(!events.is_empty());
        let journal = serde_json::to_string(&*events).unwrap();
        assert!(!journal.contains("4111111111111111"), "{journal}");
        assert!(!journal.contains("was seen"), "{journal}");
    }
}

#[tokio::test]
async fn a_guarded_session_passes_messages_to_a_human_and_withholds_them_from_an_agent() {
    for (principal, expected) in [(human(), 1), (agent(), 0)] {
        let audit = Arc::new(RecordingAudit::default());
        let mut session = guard_for(principal, audit).open_session().await.unwrap();
        let batch = session
            .query_result_sets_params_controlled("SELECT 1", &[], &control())
            .await
            .unwrap();
        assert_eq!(batch.messages.len(), expected);
    }
}
