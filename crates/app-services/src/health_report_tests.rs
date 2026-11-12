use std::sync::Mutex;

use async_trait::async_trait;
use bookie_core::{
    ColumnInfo, Connection, DriverError, ExecResult, HEALTH_ROW_LIMIT, HealthReport, OperationControl, QueryResult,
    QueryRows, TableInfo, Value,
};
use tokio_util::sync::CancellationToken;

use super::*;

enum Behaviour {
    Rows(QueryResult),
    Stall,
}

struct Fake {
    behaviour: Behaviour,
    queries: Mutex<Vec<(String, Vec<Value>)>>,
    writes: Mutex<usize>,
}

impl Fake {
    fn new(behaviour: Behaviour) -> Self {
        Self {
            behaviour,
            queries: Mutex::new(Vec::new()),
            writes: Mutex::new(0),
        }
    }
}

#[async_trait]
impl Connection for Fake {
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

    async fn query_params(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DriverError> {
        self.queries.lock().unwrap().push((sql.to_owned(), params.to_vec()));
        match &self.behaviour {
            Behaviour::Rows(result) => Ok(result.clone()),
            Behaviour::Stall => std::future::pending().await,
        }
    }

    async fn execute(&self, _sql: &str) -> Result<ExecResult, DriverError> {
        *self.writes.lock().unwrap() += 1;
        Ok(ExecResult { rows_affected: 0 })
    }

    async fn execute_params(&self, _sql: &str, _params: &[Value]) -> Result<ExecResult, DriverError> {
        *self.writes.lock().unwrap() += 1;
        Ok(ExecResult { rows_affected: 0 })
    }

    async fn execute_in_transaction(&self, _statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        *self.writes.lock().unwrap() += 1;
        Ok(Vec::new())
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "text".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn result(names: &[&str], rows: Vec<Vec<Value>>) -> QueryResult {
    let mut set = QueryRows::new();
    for row in rows {
        set.push(row);
    }
    QueryResult {
        columns: names.iter().map(|name| column(name)).collect(),
        rows: set,
        truncated: false,
    }
}

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(5))
}

#[test]
fn every_report_classifies_as_one_read_statement() {
    for report in HealthReport::ALL {
        let facts = bookie_policy::classify(&report.sql(), "postgres");
        assert!(facts.is_single_read(), "{report:?}");
    }
}

#[tokio::test]
async fn a_report_runs_its_fixed_query_with_the_hidden_schemas_as_a_parameter() {
    let fake = Fake::new(Behaviour::Rows(result(&["schema_name", "table_name"], Vec::new())));
    let hidden = vec!["archive".to_owned()];
    run_health_report(&fake, HealthReport::UnusedIndexes, &hidden, &control())
        .await
        .unwrap();
    let queries = fake.queries.lock().unwrap();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].0, HealthReport::UnusedIndexes.sql());
    assert_eq!(queries[0].1, vec![Value::Text("[\"archive\"]".into())]);
    assert_eq!(*fake.writes.lock().unwrap(), 0);
}

#[tokio::test]
async fn a_vacuum_row_offers_a_statement_but_never_runs_it() {
    let columns = [
        "schema_name",
        "table_name",
        "total_bytes",
        "total_size",
        "dead_rows",
        "last_vacuum",
    ];
    let row = vec![
        Value::Text("public".into()),
        Value::Text("orders".into()),
        Value::Int(1_000_000),
        Value::Text("977 kB".into()),
        Value::Int(42),
        Value::Null,
    ];
    let fake = Fake::new(Behaviour::Rows(result(&columns, vec![row])));
    let rows = run_health_report(&fake, HealthReport::TableVacuum, &[], &control())
        .await
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    let first = &rows.rows[0];
    assert_eq!(first.title, "public.orders");
    assert_eq!(
        first.diagnostic_sql.as_deref(),
        Some("VACUUM (ANALYZE) \"public\".\"orders\"")
    );
    assert!(first.summary.contains("977 kB") && first.summary.contains("42"));
    assert!(!first.summary.contains("1000000"), "raw byte counts are not repeated");
    assert!(!rows.may_have_more);
    assert_eq!(*fake.writes.lock().unwrap(), 0);
}

#[tokio::test]
async fn an_index_row_is_titled_by_the_index_and_names_its_table() {
    let columns = ["schema_name", "index_name", "table_name", "index_size", "scans"];
    let row = vec![
        Value::Text("public".into()),
        Value::Text("orders_old_idx".into()),
        Value::Text("orders".into()),
        Value::Text("8192 bytes".into()),
        Value::Int(0),
    ];
    let fake = Fake::new(Behaviour::Rows(result(&columns, vec![row])));
    let rows = run_health_report(&fake, HealthReport::UnusedIndexes, &[], &control())
        .await
        .unwrap();
    assert_eq!(rows.rows[0].title, "public.orders_old_idx");
    assert!(rows.rows[0].summary.contains("orders"));
    assert_eq!(
        rows.rows[0].diagnostic_sql.as_deref(),
        Some("DROP INDEX CONCURRENTLY IF EXISTS \"public\".\"orders_old_idx\"")
    );
}

#[tokio::test]
async fn a_full_page_says_more_rows_may_exist() {
    let columns = ["schema_name", "table_name"];
    let rows: Vec<Vec<Value>> = (0..HEALTH_ROW_LIMIT)
        .map(|index| vec![Value::Text("s".into()), Value::Text(format!("t{index}"))])
        .collect();
    let fake = Fake::new(Behaviour::Rows(result(&columns, rows)));
    let shown = run_health_report(&fake, HealthReport::SeqScanHeavy, &[], &control())
        .await
        .unwrap();
    assert!(shown.may_have_more);
    assert!(shown.rows.iter().all(|row| row.diagnostic_sql.is_none()));
}

#[tokio::test]
async fn a_stalled_server_is_cut_short_by_cancel_and_by_the_deadline() {
    let fake = Fake::new(Behaviour::Stall);
    let token = CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();
    });
    let cancelled = run_health_report(
        &fake,
        HealthReport::TableVacuum,
        &[],
        &OperationControl::new(token, None),
    )
    .await;
    assert!(
        matches!(&cancelled, Err(DriverError::OperationOutcomeUnknown { source }) if matches!(**source, DriverError::Cancelled)),
        "{cancelled:?}"
    );

    let deadline = OperationControl::with_timeout(std::time::Duration::from_millis(50));
    let timed_out = run_health_report(&fake, HealthReport::TableVacuum, &[], &deadline).await;
    assert!(
        matches!(&timed_out, Err(DriverError::OperationOutcomeUnknown { source }) if matches!(**source, DriverError::TimedOut)),
        "{timed_out:?}"
    );
}
