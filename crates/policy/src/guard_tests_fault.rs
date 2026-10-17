use super::*;
use crate::approval::AutoApproveSink;
use crate::guard::ConnectionFaultSink;

struct PanickingStatusConn;

#[async_trait]
impl Connection for PanickingStatusConn {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_columns(&self, _: Option<&str>, _: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        Ok(Vec::new())
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
        Ok(Vec::new())
    }

    async fn open_session(&self) -> Result<Box<dyn tablepro_core::Session>, DriverError> {
        Ok(Box::new(PanickingStatusSession))
    }

    fn supports_server_cancellation(&self) -> bool {
        panic!("secret capability panic")
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct PanickingStatusSession;

#[async_trait]
impl tablepro_core::Session for PanickingStatusSession {
    async fn query_params_controlled(
        &mut self,
        _: &str,
        _: &[Value],
        _: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        Ok(empty_result())
    }

    fn is_usable(&self) -> bool {
        panic!("secret session status panic")
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct RecordingFault {
    pub(super) operations: Mutex<Vec<String>>,
}

impl ConnectionFaultSink for RecordingFault {
    fn connection_became_unusable(&self, operation: &str) {
        self.operations.lock().expect("fault lock").push(operation.to_string());
    }
}

fn guard_with_fault(connection: Arc<dyn Connection>, fault: Arc<RecordingFault>) -> PolicyGuard {
    PolicyGuard::new(
        connection,
        context(
            Principal::human_gui(),
            Environment::Prod,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    )
    .with_fault_sink(fault)
}

#[test]
fn a_panicking_connection_capability_is_contained_and_reports_the_connection() {
    let fault = Arc::new(RecordingFault::default());
    let guard = guard_with_fault(Arc::new(PanickingStatusConn), fault.clone());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| guard.supports_server_cancellation()));

    assert!(!result.expect("capability panic is contained"));
    assert_eq!(
        *fault.operations.lock().expect("fault lock"),
        vec!["SERVER CANCELLATION CAPABILITY"]
    );
}

#[tokio::test]
async fn a_panicking_session_health_check_is_contained_and_reports_the_connection() {
    let fault = Arc::new(RecordingFault::default());
    let guard = guard_with_fault(Arc::new(PanickingStatusConn), fault.clone());
    let session = guard.open_session().await.expect("open session");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| session.is_usable()));

    assert!(!result.expect("session status panic is contained"));
    assert_eq!(*fault.operations.lock().expect("fault lock"), vec!["SESSION STATUS"]);
}

#[tokio::test]
async fn a_panicking_read_reports_the_connection_unusable() {
    let fault = Arc::new(RecordingFault::default());
    let guard = guard_with_fault(Arc::new(PanickingConn), fault.clone());

    let error = guard.list_tables().await.expect_err("panic becomes an error");

    assert!(matches!(error, DriverError::Internal(_)));
    assert!(!error.to_string().contains("codec read past the end"));
    assert_eq!(*fault.operations.lock().expect("fault lock"), vec!["LIST TABLES"]);
}

#[tokio::test]
async fn a_panicking_write_reports_the_connection_unusable() {
    let fault = Arc::new(RecordingFault::default());
    let guard = guard_with_fault(Arc::new(PanickingConn), fault.clone());

    let error = guard
        .execute("INSERT INTO jobs(id) VALUES (1)")
        .await
        .expect_err("panic becomes an error");

    assert!(matches!(error, DriverError::OperationOutcomeUnknown { .. }));
    assert_eq!(fault.operations.lock().expect("fault lock").len(), 1);
}

#[tokio::test]
async fn a_disconnected_read_reports_the_connection_unusable() {
    let fault = Arc::new(RecordingFault::default());
    let guard = guard_with_fault(Arc::new(DisconnectedConn::default()), fault.clone());

    let error = guard.list_tables().await.expect_err("disconnected is surfaced");

    assert!(matches!(error, DriverError::Disconnected));
    assert_eq!(*fault.operations.lock().expect("fault lock"), vec!["LIST TABLES"]);
}

#[tokio::test]
async fn a_healthy_read_reports_nothing() {
    let fault = Arc::new(RecordingFault::default());
    let guard = guard_with_fault(Arc::new(AmbiguousWriteConn), fault.clone());

    guard.list_tables().await.expect("healthy read");

    assert!(fault.operations.lock().expect("fault lock").is_empty());
}

fn codec_panic() -> DriverError {
    panic!("codec read past the end of the buffer")
}

#[tokio::test]
async fn a_panicking_blast_radius_count_is_contained_and_reports_the_connection() {
    let fault = Arc::new(RecordingFault::default());
    let guard = guard_with_fault(Arc::new(ErrorConn(codec_panic)), fault.clone());

    let error = guard
        .execute("UPDATE jobs SET status = 'done' WHERE id = 1")
        .await
        .expect_err("the driver also panics on the write itself");

    assert!(
        matches!(error, DriverError::OperationOutcomeUnknown { .. }),
        "{error:?}"
    );
    let reported = fault.operations.lock().expect("fault lock").clone();
    assert_eq!(
        reported.first().map(String::as_str),
        Some("BLAST RADIUS COUNT"),
        "{reported:?}"
    );
}
