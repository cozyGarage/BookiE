use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tablepro_core::{
    ColumnInfo, Connection, DriverCapabilities, DriverError, ExecResult, ForeignKeyInfo, OperationControl, QueryResult,
    TableInfo, Value,
};
use tokio_util::sync::CancellationToken;

use super::read::{MAX_TABLES, read_schema};

#[derive(Default)]
struct Calls {
    columns: AtomicUsize,
    keys: AtomicUsize,
    cancel_at: AtomicUsize,
    cancellation: CancellationToken,
}

struct Catalog {
    calls: Arc<Calls>,
    failing: Vec<&'static str>,
    keys: HashMap<&'static str, Vec<ForeignKeyInfo>>,
}

fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "integer".into(),
        nullable: false,
        primary_key: name == "id",
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn table(name: &str) -> TableInfo {
    TableInfo {
        schema: Some("shop".into()),
        name: name.into(),
    }
}

#[async_trait]
impl Connection for Catalog {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_columns(&self, _schema: Option<&str>, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        let seen = self.calls.columns.fetch_add(1, Ordering::SeqCst) + 1;
        if seen == self.calls.cancel_at.load(Ordering::SeqCst) {
            self.calls.cancellation.cancel();
        }
        if self.failing.contains(&table) {
            return Err(DriverError::query("permission denied"));
        }
        Ok(vec![column("id")])
    }

    async fn fetch_rows(&self, _: Option<&str>, _: &str, _: u64, _: u64) -> Result<QueryResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn query(&self, _sql: &str) -> Result<QueryResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn execute(&self, _sql: &str) -> Result<ExecResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
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

    async fn fetch_foreign_keys(&self, _schema: Option<&str>, table: &str) -> Result<Vec<ForeignKeyInfo>, DriverError> {
        self.calls.keys.fetch_add(1, Ordering::SeqCst);
        Ok(self.keys.get(table).cloned().unwrap_or_default())
    }
}

fn catalog(calls: &Arc<Calls>, failing: Vec<&'static str>) -> Catalog {
    Catalog {
        calls: calls.clone(),
        failing,
        keys: HashMap::new(),
    }
}

fn capabilities(foreign_keys: bool) -> DriverCapabilities {
    DriverCapabilities {
        supports_foreign_key_metadata: foreign_keys,
        ..DriverCapabilities::default()
    }
}

fn control(calls: &Calls) -> OperationControl {
    OperationControl::new(calls.cancellation.clone(), None)
}

#[tokio::test]
async fn every_table_is_read_in_name_order_with_progress() {
    let calls = Arc::new(Calls::default());
    let connection = catalog(&calls, Vec::new());
    let tables = [table("orders"), table("customers")];
    let mut progress = Vec::new();

    let read = read_schema(
        &connection,
        Some("shop"),
        &tables,
        capabilities(true),
        &control(&calls),
        |done, total| progress.push((done, total)),
    )
    .await
    .unwrap();

    let names: Vec<&str> = read.reads.iter().map(|read| read.name.as_str()).collect();
    assert_eq!(names, ["customers", "orders"]);
    assert_eq!(progress, [(1, 2), (2, 2)]);
    assert_eq!(calls.keys.load(Ordering::SeqCst), 2);
    assert!(read.foreign_keys_supported);
    assert!(read.skipped.is_empty());
    assert!(!read.truncated());
}

#[tokio::test]
async fn an_engine_without_foreign_key_metadata_reads_no_keys_and_says_so() {
    let calls = Arc::new(Calls::default());
    let connection = catalog(&calls, Vec::new());

    let read = read_schema(
        &connection,
        None,
        &[table("items")],
        capabilities(false),
        &control(&calls),
        |_, _| {},
    )
    .await
    .unwrap();

    assert!(!read.foreign_keys_supported);
    assert_eq!(calls.keys.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_table_the_account_cannot_read_is_skipped_and_named() {
    let calls = Arc::new(Calls::default());
    let connection = catalog(&calls, vec!["secrets"]);

    let read = read_schema(
        &connection,
        Some("shop"),
        &[table("secrets"), table("items")],
        capabilities(true),
        &control(&calls),
        |_, _| {},
    )
    .await
    .unwrap();

    assert_eq!(read.reads.len(), 1);
    assert_eq!(read.skipped, ["secrets"]);
}

#[tokio::test]
async fn a_large_schema_is_cut_at_the_table_limit_and_reports_the_total() {
    let calls = Arc::new(Calls::default());
    let connection = catalog(&calls, Vec::new());
    let tables: Vec<TableInfo> = (0..MAX_TABLES + 25).map(|at| table(&format!("t{at:04}"))).collect();

    let read = read_schema(
        &connection,
        Some("shop"),
        &tables,
        capabilities(true),
        &control(&calls),
        |_, _| {},
    )
    .await
    .unwrap();

    assert_eq!(read.reads.len(), MAX_TABLES);
    assert_eq!(read.total_tables, MAX_TABLES + 25);
    assert!(read.truncated());
    assert_eq!(calls.columns.load(Ordering::SeqCst), MAX_TABLES);
}

#[tokio::test]
async fn cancelling_stops_the_read_before_the_next_table() {
    let calls = Arc::new(Calls::default());
    calls.cancel_at.store(2, Ordering::SeqCst);
    let connection = catalog(&calls, Vec::new());
    let tables = [table("a"), table("b"), table("c"), table("d")];

    let result = read_schema(
        &connection,
        Some("shop"),
        &tables,
        capabilities(true),
        &control(&calls),
        |_, _| {},
    )
    .await;

    assert!(matches!(result, Err(DriverError::Cancelled)), "{result:?}");
    assert_eq!(calls.columns.load(Ordering::SeqCst), 2);
}
