use std::sync::Mutex;

use async_trait::async_trait;
use bookie_core::{ColumnInfo, Connection, DriverError, ExecResult, OperationControl, QueryResult, TableInfo, Value};

use super::{SetSequenceError, parse_sequence_value, set_sequence_value};

struct Recorder {
    statements: Mutex<Vec<String>>,
    denial: Option<&'static str>,
}

impl Recorder {
    fn new(denial: Option<&'static str>) -> Self {
        Self {
            statements: Mutex::new(Vec::new()),
            denial,
        }
    }
}

#[async_trait]
impl Connection for Recorder {
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
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn query(&self, _sql: &str) -> Result<QueryResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn execute(&self, sql: &str) -> Result<ExecResult, DriverError> {
        if let Some(reason) = self.denial {
            return Err(DriverError::PolicyDenied(reason.into()));
        }
        self.statements.lock().unwrap().push(sql.to_owned());
        Ok(ExecResult { rows_affected: 0 })
    }

    async fn execute_params(&self, _sql: &str, _params: &[Value]) -> Result<ExecResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn execute_in_transaction(&self, _statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(5))
}

#[test]
fn only_whole_numbers_are_accepted() {
    assert_eq!(parse_sequence_value(" 42 ").unwrap(), 42);
    assert_eq!(parse_sequence_value("-7").unwrap(), -7);
    for text in ["", "4.5", "1e3", "ten", "42; DROP TABLE t", "9223372036854775808"] {
        assert!(
            matches!(parse_sequence_value(text), Err(SetSequenceError::NotANumber)),
            "{text}"
        );
    }
}

#[tokio::test]
async fn setting_a_value_runs_one_restart_statement_through_the_connection() {
    let connection = Recorder::new(None);

    set_sequence_value(&connection, "postgres", Some("public"), "job_ids", "42", &control())
        .await
        .unwrap();

    assert_eq!(
        *connection.statements.lock().unwrap(),
        ["ALTER SEQUENCE \"public\".\"job_ids\" RESTART WITH 42"]
    );
}

#[tokio::test]
async fn an_invalid_value_runs_nothing() {
    let connection = Recorder::new(None);

    let error = set_sequence_value(&connection, "postgres", None, "job_ids", "abc", &control())
        .await
        .unwrap_err();

    assert!(matches!(error, SetSequenceError::NotANumber));
    assert!(connection.statements.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_engine_without_a_restart_statement_runs_nothing() {
    let connection = Recorder::new(None);

    let error = set_sequence_value(&connection, "mysql", None, "job_ids", "1", &control())
        .await
        .unwrap_err();

    assert!(matches!(error, SetSequenceError::Statement(_)));
    assert!(connection.statements.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_policy_denial_reaches_the_caller_unchanged() {
    let connection = Recorder::new(Some("read-only connection"));

    let error = set_sequence_value(&connection, "postgres", None, "job_ids", "1", &control())
        .await
        .unwrap_err();

    assert!(
        matches!(error, SetSequenceError::Driver(DriverError::PolicyDenied(_))),
        "{error:?}"
    );
}
