use super::*;
use crate::approval::AutoApproveSink;
use crate::audit::{AuditRecordPhase, AuditTerminalStatus};
use std::time::Duration;
use tablepro_core::Session;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

fn guard(connection: Arc<dyn Connection>, audit: Arc<SequenceAuditSink>, read_only: bool) -> PolicyGuard {
    let mut context = super::context(
        Principal::human_gui(),
        Environment::Prod,
        PolicyConfig::default(),
        Arc::new(AutoApproveSink),
        audit,
        Arc::new(AuditState::new()),
    );
    context.read_only = read_only;
    PolicyGuard::new(connection, context)
}

#[tokio::test]
async fn a_driver_failure_carries_the_id_of_its_recorded_audit_operation() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = guard(Arc::new(ErrorConn(super::query_error)), audit.clone(), false);
    let control = OperationControl::with_timeout(Duration::from_secs(5));

    let outcome = guard
        .query_result_sets_controlled_with_audit("SELECT 1", &[], &control)
        .await;

    assert!(matches!(outcome.result, Err(DriverError::Query { .. })));
    assert_eq!(outcome.audit_receipts.len(), 1);
    let receipt = outcome.audit_receipts.first().copied().expect("intent was recorded");
    let events = audit.events.lock().expect("audit events");
    assert!(!events.is_empty());
    assert!(events.iter().all(|event| event.operation_id == receipt.operation_id));
}

#[tokio::test]
async fn a_guarded_session_query_carries_its_own_recorded_operation_id() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = guard(
        Arc::new(InterruptedQueryConn {
            started: Arc::new(Notify::new()),
        }),
        audit.clone(),
        false,
    );
    let mut session = guard.open_session_with_audit().await.expect("session");
    let control = OperationControl::with_timeout(Duration::from_secs(5));

    let outcome = session
        .query_result_sets_params_controlled_with_audit("SELECT 1", &[], &control)
        .await;

    assert!(matches!(outcome.result, Err(DriverError::Query { .. })));
    let receipt = outcome
        .audit_receipts
        .first()
        .copied()
        .expect("session query was audited");
    let events = audit.events.lock().expect("audit events");
    assert!(events.iter().all(|event| event.operation_id == receipt.operation_id));
}

#[tokio::test]
async fn a_refusal_carries_its_recorded_id_without_dispatching() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = guard(Arc::new(ErrorConn(super::query_error)), audit.clone(), true);
    let control = OperationControl::with_timeout(Duration::from_secs(5));

    let outcome = guard
        .query_result_sets_controlled_with_audit("DELETE FROM jobs", &[], &control)
        .await;

    assert!(matches!(outcome.result, Err(DriverError::PolicyDenied(_))));
    let receipt = outcome.audit_receipts.first().copied().expect("denial was recorded");
    let events = audit.events.lock().expect("audit events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].operation_id, receipt.operation_id);
    assert_eq!(events[0].terminal_status, AuditTerminalStatus::Denied);
}

#[tokio::test]
async fn a_failed_audit_intent_does_not_return_a_linkable_id() {
    let audit = Arc::new(SequenceAuditSink::new(vec![AuditRecordPhase::Intent]));
    let guard = guard(Arc::new(ErrorConn(super::query_error)), audit.clone(), false);
    let control = OperationControl::with_timeout(Duration::from_secs(5));

    let outcome = guard
        .query_result_sets_controlled_with_audit("SELECT 1", &[], &control)
        .await;

    assert!(outcome.result.is_err());
    assert!(outcome.audit_receipts.is_empty());
    assert!(audit.events.lock().expect("audit events").is_empty());
}

#[tokio::test]
async fn a_persisted_intent_remains_linkable_when_recording_its_outcome_fails() {
    let audit = Arc::new(SequenceAuditSink::new(vec![AuditRecordPhase::Outcome]));
    let guard = guard(Arc::new(ErrorConn(super::query_error)), audit.clone(), false);
    let control = OperationControl::with_timeout(Duration::from_secs(5));

    let outcome = guard
        .query_result_sets_controlled_with_audit("SELECT 1", &[], &control)
        .await;

    assert!(matches!(outcome.result, Err(DriverError::Query { .. })));
    let receipt = outcome.audit_receipts.first().copied().expect("intent was persisted");
    let events = audit.events.lock().expect("audit events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].operation_id, receipt.operation_id);
    assert_eq!(events[0].phase, AuditRecordPhase::Intent);
}

#[tokio::test]
async fn cancellation_before_dispatch_carries_its_recorded_terminal_event() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = guard(Arc::new(ErrorConn(super::query_error)), audit.clone(), false);
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let control = OperationControl::new(cancellation, None);

    let outcome = guard
        .query_result_sets_controlled_with_audit("SELECT 1", &[], &control)
        .await;

    assert!(matches!(outcome.result, Err(DriverError::Cancelled)));
    let receipt = outcome
        .audit_receipts
        .first()
        .copied()
        .expect("cancellation was recorded");
    let events = audit.events.lock().expect("audit events");
    assert!(events.iter().any(|event| {
        event.operation_id == receipt.operation_id && event.terminal_status == AuditTerminalStatus::Cancelled
    }));
}

#[tokio::test]
async fn concurrent_failures_return_distinct_recorded_ids() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let guard = guard(Arc::new(ErrorConn(super::query_error)), audit.clone(), false);
    let first_control = OperationControl::with_timeout(Duration::from_secs(5));
    let second_control = OperationControl::with_timeout(Duration::from_secs(5));

    let (first, second) = tokio::join!(
        guard.query_result_sets_controlled_with_audit("SELECT 1", &[], &first_control),
        guard.query_result_sets_controlled_with_audit("SELECT 2", &[], &second_control),
    );

    let first_id = first
        .audit_receipts
        .first()
        .expect("first intent was recorded")
        .operation_id;
    let second_id = second
        .audit_receipts
        .first()
        .expect("second intent was recorded")
        .operation_id;
    assert_ne!(first_id, second_id);
    let events = audit.events.lock().expect("audit events");
    assert!(events.iter().any(|event| event.operation_id == first_id));
    assert!(events.iter().any(|event| event.operation_id == second_id));
}

struct InterruptedQueryConn {
    started: Arc<Notify>,
}

#[async_trait]
impl Connection for InterruptedQueryConn {
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

    async fn query_controlled(&self, _sql: &str, control: &OperationControl) -> Result<QueryResult, DriverError> {
        self.started.notify_one();
        if let Some(deadline) = control.deadline() {
            tokio::time::sleep_until(deadline).await;
            return Err(DriverError::TimedOut);
        }
        control.cancellation_token().cancelled().await;
        Err(DriverError::Cancelled)
    }

    async fn execute(&self, _sql: &str) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 0 })
    }

    async fn execute_params(&self, _sql: &str, _params: &[Value]) -> Result<ExecResult, DriverError> {
        self.execute("").await
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

    async fn open_session(&self) -> Result<Box<dyn Session>, DriverError> {
        Ok(Box::new(FailedQuerySession))
    }
}

struct FailedQuerySession;

#[async_trait]
impl Session for FailedQuerySession {
    async fn query_params_controlled(
        &mut self,
        _sql: &str,
        _params: &[Value],
        _control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        Err(super::query_error())
    }

    fn is_usable(&self) -> bool {
        true
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

#[tokio::test]
async fn a_timeout_after_audit_intent_keeps_the_operation_receipt() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let started = Arc::new(Notify::new());
    let guard = guard(
        Arc::new(InterruptedQueryConn {
            started: started.clone(),
        }),
        audit.clone(),
        false,
    );
    let control = OperationControl::with_timeout(Duration::from_millis(20));

    let outcome = guard
        .query_result_sets_controlled_with_audit("SELECT 1", &[], &control)
        .await;

    assert!(matches!(outcome.result, Err(DriverError::TimedOut)));
    let receipt = outcome
        .audit_receipts
        .first()
        .copied()
        .expect("intent was recorded before timeout");
    let events = audit.events.lock().expect("audit events");
    assert!(events.iter().any(|event| {
        event.operation_id == receipt.operation_id && event.terminal_status == AuditTerminalStatus::TimedOut
    }));
}

#[tokio::test]
async fn cancellation_after_audit_intent_keeps_the_operation_receipt() {
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let started = Arc::new(Notify::new());
    let guard = guard(
        Arc::new(InterruptedQueryConn {
            started: started.clone(),
        }),
        audit.clone(),
        false,
    );
    let cancellation = CancellationToken::new();
    let control = OperationControl::new(cancellation.clone(), None);
    let request = tokio::spawn(async move {
        guard
            .query_result_sets_controlled_with_audit("SELECT 1", &[], &control)
            .await
    });

    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .expect("query dispatched");
    cancellation.cancel();
    let outcome = request.await.expect("guarded query finished");

    assert!(matches!(outcome.result, Err(DriverError::Cancelled)));
    let receipt = outcome
        .audit_receipts
        .first()
        .copied()
        .expect("intent was recorded before cancellation");
    let events = audit.events.lock().expect("audit events");
    assert!(events.iter().any(|event| {
        event.operation_id == receipt.operation_id && event.terminal_status == AuditTerminalStatus::Cancelled
    }));
}
