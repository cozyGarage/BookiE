use super::*;

struct CountingTransaction {
    count: i64,
}

#[async_trait]
impl Transaction for CountingTransaction {
    async fn query(&mut self, sql: &str) -> Result<QueryResult, DriverError> {
        let mut result = empty_result();
        if sql.to_ascii_lowercase().contains("count(") {
            result.rows = vec![vec![Value::Int(self.count)]];
        }
        Ok(result)
    }

    async fn execute(&mut self, _: &str) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 1 })
    }

    async fn commit(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct TransactionConn {
    count: i64,
}

#[async_trait]
impl Connection for TransactionConn {
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
        Ok(empty_result())
    }
    async fn execute(&self, _: &str) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 0 })
    }
    async fn execute_params(&self, _: &str, _: &[Value]) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 0 })
    }
    async fn execute_in_transaction(&self, _: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Ok(vec![])
    }
    async fn begin(&self) -> Result<Box<dyn Transaction>, DriverError> {
        Ok(Box::new(CountingTransaction { count: self.count }))
    }
    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }
    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

async fn approvals_for(count_in_transaction: i64) -> usize {
    let approvals = Arc::new(AtomicUsize::new(0));
    let ctx = context(
        Principal::human_gui(),
        Environment::Staging,
        PolicyConfig::default(),
        Arc::new(CountingApprovalSink {
            calls: approvals.clone(),
        }),
        Arc::new(SequenceAuditSink::new(vec![])),
        Arc::new(AuditState::new()),
    );
    let guard = PolicyGuard::new(
        Arc::new(TransactionConn {
            count: count_in_transaction,
        }),
        ctx,
    );
    let mut transaction = guard.begin().await.expect("begin");
    transaction
        .execute("UPDATE items SET flag = 1 WHERE flag IS NULL")
        .await
        .expect("an approved write runs");
    approvals.load(Ordering::SeqCst)
}

#[tokio::test]
async fn the_blast_radius_is_counted_inside_the_transaction_that_will_run_the_write() {
    assert_eq!(
        approvals_for(1).await,
        0,
        "a small count seen by the transaction needs no approval"
    );
    assert_eq!(
        approvals_for(6_000).await,
        1,
        "a count above the limit asks for approval"
    );
}
