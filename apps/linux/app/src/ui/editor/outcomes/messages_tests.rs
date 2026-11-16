use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use bookie_core::sql_syntax::script::BatchErrorPolicy;
use bookie_core::{
    ColumnInfo, Connection, DriverError, ExecResult, MessageSeverity, OperationControl, QueryResult, QueryResultBatch,
    ServerMessage, ServerMessages, TableInfo, Value,
};

use super::super::session_mode::StatementTarget;
use super::{ScriptRunResult, run_statements};

struct NoisyOnce {
    calls: AtomicUsize,
}

fn empty() -> QueryResult {
    QueryResult {
        columns: Vec::new(),
        rows: Vec::new().into(),
        truncated: false,
    }
}

#[async_trait]
impl Connection for NoisyOnce {
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
        Ok(empty())
    }

    async fn query(&self, _sql: &str) -> Result<QueryResult, DriverError> {
        Ok(empty())
    }

    async fn query_result_sets_controlled(
        &self,
        _sql: &str,
        _params: &[Value],
        _control: &OperationControl,
    ) -> Result<QueryResultBatch, DriverError> {
        let mut messages = ServerMessages::default();
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            messages.push(ServerMessage::new(
                MessageSeverity::Warning,
                Some("1292".into()),
                "first only",
            ));
        }
        Ok(QueryResultBatch::single(empty()).with_messages(messages))
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

#[tokio::test]
async fn each_statement_outcome_keeps_only_its_own_server_messages() {
    let connection: Arc<dyn Connection> = Arc::new(NoisyOnce {
        calls: AtomicUsize::new(0),
    });
    let control = crate::services::operation_control::bounded(0);

    let result = run_statements(
        StatementTarget::Pool(connection),
        vec!["SELECT 1".into(), "SELECT 2".into()],
        "mysql",
        &Default::default(),
        &control,
        BatchErrorPolicy::StopScript,
        |_| {},
    )
    .await;

    let ScriptRunResult::Completed(outcomes) = result else {
        panic!("expected the run to complete")
    };
    let first: Vec<_> = outcomes[0].detail.messages.iter().collect();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].text, "first only");
    assert_eq!(first[0].code.as_deref(), Some("1292"));
    assert!(outcomes[1].detail.messages.is_empty());
}
