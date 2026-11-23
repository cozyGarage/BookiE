use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use bookie_core::{
    ColumnInfo, Connection, DriverCapabilities, DriverError, ExecResult, ForeignKeyInfo, OperationControl, QueryResult,
    SchemaTableMetadataVisitor, TableInfo, Value,
};
use tokio_util::sync::CancellationToken;

use super::read::{MAX_TABLES, read_schema, read_schema_with_limit};

#[derive(Default)]
struct Calls {
    schema_metadata: AtomicUsize,
    columns: AtomicUsize,
    keys: AtomicUsize,
    cancel_at: AtomicUsize,
    batch_error_after: AtomicUsize,
    cancellation: CancellationToken,
    column_schemas: Mutex<Vec<Option<String>>>,
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

fn table_in(schema: Option<&str>, name: &str) -> TableInfo {
    TableInfo {
        schema: schema.map(str::to_string),
        name: name.to_string(),
    }
}

#[async_trait]
impl Connection for Catalog {
    async fn fetch_schema_table_metadata_controlled(
        &self,
        tables: &[TableInfo],
        include_foreign_keys: bool,
        control: &OperationControl,
        visit: &mut SchemaTableMetadataVisitor<'_>,
    ) -> Result<(), DriverError> {
        self.calls.schema_metadata.fetch_add(1, Ordering::SeqCst);
        for (index, table) in tables.iter().enumerate() {
            let columns = match self
                .fetch_columns_controlled(table.schema.as_deref(), &table.name, control)
                .await
            {
                Ok(columns) => columns,
                Err(error) => {
                    if !visit(index, Err(error)) {
                        break;
                    }
                    continue;
                }
            };
            let foreign_keys = if include_foreign_keys {
                match self
                    .fetch_foreign_keys_controlled(table.schema.as_deref(), &table.name, control)
                    .await
                {
                    Ok(keys) => keys,
                    Err(error) => {
                        if !visit(index, Err(error)) {
                            break;
                        }
                        continue;
                    }
                }
            } else {
                Vec::new()
            };
            if !visit(index, Ok((columns, foreign_keys))) {
                break;
            }
            if self.calls.batch_error_after.load(Ordering::SeqCst) == index + 1 {
                return Err(DriverError::query("batch stream failed"));
            }
        }
        Ok(())
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_columns(&self, schema: Option<&str>, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        self.calls
            .column_schemas
            .lock()
            .unwrap()
            .push(schema.map(str::to_string));
        let seen = self.calls.columns.fetch_add(1, Ordering::SeqCst) + 1;
        if seen == self.calls.cancel_at.load(Ordering::SeqCst) {
            self.calls.cancellation.cancel();
        }
        if self.failing.contains(&table) {
            if table == "disconnected" {
                return Err(DriverError::Disconnected);
            }
            if table == "denied" {
                return Err(DriverError::PolicyDenied("denied".into()));
            }
            if table == "a-unknown" {
                return Err(DriverError::OperationOutcomeUnknown {
                    source: Box::new(DriverError::TimedOut),
                });
            }
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
    assert_eq!(calls.schema_metadata.load(Ordering::SeqCst), 1);
    assert_eq!(calls.keys.load(Ordering::SeqCst), 2);
    assert!(read.foreign_keys_supported);
    assert!(read.skipped.is_empty());
    assert!(!read.truncated());
}

#[tokio::test]
async fn an_all_tables_diagram_reads_each_table_from_its_own_schema() {
    let calls = Arc::new(Calls::default());
    let connection = catalog(&calls, Vec::new());

    let read = read_schema(
        &connection,
        None,
        &[table_in(None, "items"), table_in(Some("aux"), "items")],
        capabilities(true),
        &control(&calls),
        |_, _| {},
    )
    .await
    .unwrap();

    let mut schemas = calls.column_schemas.lock().unwrap().clone();
    schemas.sort();
    assert_eq!(schemas, [None, Some("aux".into())]);
    assert!(read.reads.iter().any(|table| table.schema.is_none()));
    assert!(read.reads.iter().any(|table| table.schema.as_deref() == Some("aux")));
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
async fn terminal_metadata_errors_stop_diagram_loading() {
    for name in ["a-unknown", "denied", "disconnected"] {
        let calls = Arc::new(Calls::default());
        let connection = catalog(&calls, vec![name]);
        let result = read_schema(
            &connection,
            Some("shop"),
            &[table(name), table("later")],
            capabilities(true),
            &control(&calls),
            |_, _| {},
        )
        .await;
        assert!(matches!(
            result,
            Err(DriverError::Disconnected | DriverError::PolicyDenied(_) | DriverError::OperationOutcomeUnknown { .. })
        ));
        assert_eq!(calls.columns.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn a_recoverable_batch_failure_falls_back_after_the_last_emitted_table() {
    let calls = Arc::new(Calls::default());
    calls.batch_error_after.store(1, Ordering::SeqCst);
    let connection = catalog(&calls, Vec::new());
    let mut progress = Vec::new();

    let read = read_schema(
        &connection,
        Some("shop"),
        &[table("a"), table("b"), table("c")],
        capabilities(true),
        &control(&calls),
        |done, total| progress.push((done, total)),
    )
    .await
    .unwrap();

    let names: Vec<_> = read.reads.iter().map(|table| table.name.as_str()).collect();
    assert_eq!(names, ["a", "b", "c"]);
    assert_eq!(calls.columns.load(Ordering::SeqCst), 3);
    assert_eq!(progress, [(1, 3), (2, 3), (3, 3)]);
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
async fn metadata_over_the_byte_limit_is_not_retained_or_reported_complete() {
    let calls = Arc::new(Calls::default());
    let connection = catalog(&calls, Vec::new());

    let read = read_schema_with_limit(
        &connection,
        Some("shop"),
        &[table("items"), table("later")],
        capabilities(true),
        &control(&calls),
        1,
        |_, _| {},
    )
    .await
    .unwrap();

    assert!(read.reads.is_empty());
    assert!(read.metadata_limit_reached);
    assert!(read.truncated());
    assert_eq!(calls.columns.load(Ordering::SeqCst), 1);
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
