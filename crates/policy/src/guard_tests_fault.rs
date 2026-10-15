use super::*;
use crate::approval::AutoApproveSink;
use crate::guard::ConnectionFaultSink;

#[derive(Default)]
struct RecordingFault {
    operations: Mutex<Vec<String>>,
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

struct FaultyConn {
    panic_query: bool,
    panic_session_close: bool,
}

struct FaultySession {
    panic_close: bool,
}

#[async_trait]
impl tablepro_core::Session for FaultySession {
    async fn query_params_controlled(
        &mut self,
        _: &str,
        _: &[Value],
        _: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        Ok(empty_result())
    }

    fn is_usable(&self) -> bool {
        true
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        if self.panic_close {
            panic!("codec read past the end of the buffer")
        }
        Ok(())
    }
}

#[async_trait]
impl Connection for FaultyConn {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(vec![])
    }

    async fn fetch_columns(&self, _: Option<&str>, _: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        Ok(vec![])
    }

    async fn fetch_rows(&self, _: Option<&str>, _: &str, _: u64, _: u64) -> Result<QueryResult, DriverError> {
        Ok(empty_result())
    }

    async fn query(&self, _: &str) -> Result<QueryResult, DriverError> {
        if self.panic_query {
            panic!("codec read past the end of the buffer")
        }
        Ok(empty_result())
    }

    async fn execute(&self, _: &str) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 1 })
    }

    async fn execute_params(&self, sql: &str, _: &[Value]) -> Result<ExecResult, DriverError> {
        self.execute(sql).await
    }

    async fn execute_in_transaction(&self, _: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Ok(vec![])
    }

    async fn open_session(&self) -> Result<Box<dyn tablepro_core::Session>, DriverError> {
        Ok(Box::new(FaultySession {
            panic_close: self.panic_session_close,
        }))
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

#[tokio::test]
async fn a_panicking_blast_radius_count_is_contained_and_reports_the_connection() {
    let fault = Arc::new(RecordingFault::default());
    let guard = guard_with_fault(
        Arc::new(FaultyConn {
            panic_query: true,
            panic_session_close: false,
        }),
        fault.clone(),
    );

    guard
        .execute("UPDATE jobs SET status = 'done' WHERE id = 1")
        .await
        .expect("an unknown blast radius does not stop a local write");

    assert_eq!(
        *fault.operations.lock().expect("fault lock"),
        vec!["BLAST RADIUS COUNT"]
    );
}

#[tokio::test]
async fn a_panicking_session_close_is_contained_and_reports_the_connection() {
    let fault = Arc::new(RecordingFault::default());
    let guard = guard_with_fault(
        Arc::new(FaultyConn {
            panic_query: false,
            panic_session_close: true,
        }),
        fault.clone(),
    );
    let session = guard.open_session().await.expect("open session");

    let error = session.close().await.expect_err("a close panic becomes an error");

    assert!(matches!(error, DriverError::Internal(_)), "{error:?}");
    assert!(!error.to_string().contains("codec read past the end"));
    assert_eq!(*fault.operations.lock().expect("fault lock"), vec!["SESSION CLOSE"]);
}
