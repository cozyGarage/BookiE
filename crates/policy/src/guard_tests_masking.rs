use super::*;
use crate::approval::AutoApproveSink;

const SECRET: &str = "duplicate key value (card 4111111111111111)";

struct FailingConn(fn() -> DriverError);

fn leak() -> DriverError {
    DriverError::Query {
        message: SECRET.into(),
        sqlstate: Some("23505".into()),
        position: Some(7),
    }
}

#[async_trait]
impl Connection for FailingConn {
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
        Err((self.0)())
    }

    async fn execute(&self, _: &str) -> Result<ExecResult, DriverError> {
        Err((self.0)())
    }

    async fn execute_params(&self, _: &str, _: &[Value]) -> Result<ExecResult, DriverError> {
        Err((self.0)())
    }

    async fn execute_in_transaction(&self, _: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Err((self.0)())
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

fn guard_for(principal: Principal, failure: fn() -> DriverError) -> PolicyGuard {
    PolicyGuard::new(
        Arc::new(FailingConn(failure)),
        context(
            principal,
            Environment::Local,
            PolicyConfig::default(),
            Arc::new(AutoApproveSink),
            Arc::new(SequenceAuditSink::new(vec![])),
            Arc::new(AuditState::new()),
        ),
    )
}

fn agent() -> Principal {
    Principal::Agent {
        token: "test-token".into(),
        client: None,
        model: None,
    }
}

fn statements() -> Vec<(String, Vec<Value>)> {
    vec![("INSERT INTO jobs(id) VALUES (1)".to_string(), vec![])]
}

fn innermost(error: &DriverError) -> &DriverError {
    match error {
        DriverError::Transaction { source, .. }
        | DriverError::TransactionRollbackFailed { source, .. }
        | DriverError::OperationOutcomeUnknown { source } => innermost(source),
        other => other,
    }
}

fn nested_transaction() -> DriverError {
    DriverError::Transaction {
        statement_index: 2,
        source: Box::new(leak()),
    }
}

fn nested_rollback_failed() -> DriverError {
    DriverError::TransactionRollbackFailed {
        statement_index: 1,
        source: Box::new(leak()),
        rollback_error: Box::new(leak()),
    }
}

fn nested_unknown() -> DriverError {
    DriverError::OperationOutcomeUnknown {
        source: Box::new(DriverError::Transaction {
            statement_index: 0,
            source: Box::new(leak()),
        }),
    }
}

#[tokio::test]
async fn an_agent_never_sees_driver_text_however_deeply_the_error_is_nested() {
    for failure in [leak, nested_transaction, nested_rollback_failed, nested_unknown] {
        let error = guard_for(agent(), failure)
            .execute_in_transaction(&statements())
            .await
            .expect_err("the write fails");

        assert!(!format!("{error:?}").contains("4111111111111111"), "{error:?}");
        assert!(!error.to_string().contains("4111111111111111"), "{error}");
        assert!(
            matches!(innermost(&error), DriverError::Query { sqlstate: Some(state), .. } if state == "23505"),
            "the SQLSTATE stays so the agent can tell what kind of failure it was: {error:?}"
        );
    }
}

#[tokio::test]
async fn masking_keeps_the_statement_index_and_the_error_shape() {
    let error = guard_for(agent(), nested_rollback_failed)
        .execute_in_transaction(&statements())
        .await
        .unwrap_err();
    let DriverError::TransactionRollbackFailed {
        statement_index,
        rollback_error,
        ..
    } = &error
    else {
        panic!("the shape must survive masking: {error:?}");
    };
    assert_eq!(*statement_index, 1);
    assert!(!format!("{rollback_error:?}").contains("4111111111111111"));

    let unknown = guard_for(agent(), nested_unknown)
        .execute_in_transaction(&statements())
        .await
        .unwrap_err();
    assert!(
        matches!(unknown, DriverError::OperationOutcomeUnknown { .. }),
        "{unknown:?}"
    );
}

#[tokio::test]
async fn a_human_sees_the_full_driver_text_of_a_nested_error() {
    for failure in [nested_transaction, nested_rollback_failed, nested_unknown] {
        let error = guard_for(Principal::human_gui(), failure)
            .execute_in_transaction(&statements())
            .await
            .unwrap_err();

        assert!(
            matches!(innermost(&error), DriverError::Query { message, .. } if message == SECRET),
            "{error:?}"
        );
    }
}
