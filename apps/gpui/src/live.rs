use std::sync::Arc;
use std::time::Duration;

use bookie_app_services::read_session::{ReadSession, list_saved_connections};
use bookie_core::{ColumnInfo, OperationControl, QueryResult, QueryResultBudget, TableInfo};
use bookie_results::result_scan::ScanSource;
use bookie_results::stream_result_spool::StreamResultSpoolManager;
use tokio_util::sync::CancellationToken;

use crate::model::{Cell, ColumnMeta, Diff, FindResult, PAGE_ROWS, Predicate};
use crate::source::{ResultSource, filter_in, find_in};

pub fn saved_connections() -> Result<Vec<bookie_app_services::read_session::ConnectionSummary>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(list_saved_connections())
}

#[derive(Clone, Debug)]
pub enum Origin {
    Saved(String),
    File(std::path::PathBuf),
}

pub const ROW_CAP: u64 = 100_000;

const PAGE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct LiveSession {
    runtime: tokio::runtime::Runtime,
    session: Option<ReadSession>,
    pub driver_id: String,
    pub label: String,
    pub tables: Vec<String>,
}

impl LiveSession {
    pub fn open(origin: &Origin) -> Result<Arc<LiveSession>, String> {
        Self::open_controlled(origin, &CancellationToken::new())
    }

    pub fn open_controlled(origin: &Origin, cancellation: &CancellationToken) -> Result<Arc<LiveSession>, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let (session, label) = runtime.block_on(open_session(origin, cancellation.clone()))?;
        let tables = session.tables.iter().map(table_label).collect();
        Ok(Arc::new(LiveSession {
            driver_id: session.driver_id.clone(),
            runtime,
            session: Some(session),
            label,
            tables,
        }))
    }

    pub fn run_query(self: &Arc<Self>, sql: &str, max_rows: u64) -> Result<LiveTable, String> {
        self.run_query_controlled(sql, max_rows, &CancellationToken::new())
    }

    pub fn run_query_controlled(
        self: &Arc<Self>,
        sql: &str,
        max_rows: u64,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, String> {
        if max_rows == 0 || max_rows > ROW_CAP {
            return Err(format!("Row limit must be between 1 and {ROW_CAP}."));
        }
        let session = self.session.as_ref().ok_or("the session is closed")?;
        let (columns, rows, scan, truncated) =
            self.runtime
                .block_on(query_pages(session, sql, max_rows, cancellation))?;
        Ok(LiveTable {
            session: self.clone(),
            columns,
            rows,
            scan,
            truncated,
            table: "query".into(),
        })
    }

    pub fn overview(&self) -> Vec<bookie_app_services::database_service::ConnectionOverview> {
        self.session.as_ref().map(ReadSession::overview).unwrap_or_default()
    }

    pub fn sample_pubsub(
        &self,
        patterns: &[String],
        max_messages: usize,
        window: Duration,
        cancellation: &CancellationToken,
    ) -> Result<drivers_registry::PubSubSample, String> {
        let session = self.session.as_ref().ok_or("the session is closed")?;
        let registry = drivers_registry::default_registry();
        let deadline = tokio::time::Instant::now() + window + Duration::from_secs(15);
        let control = OperationControl::new(cancellation.clone(), Some(deadline));
        self.runtime
            .block_on(session.with_subscription(&registry, &control, |options| {
                drivers_registry::sample_pubsub(options, patterns, max_messages, window)
            }))
            .map_err(|failure| format!("{failure:?}"))
    }

    pub fn load_table(self: &Arc<Self>, table: &str, max_rows: u64) -> Result<LiveTable, String> {
        self.load_table_controlled(table, max_rows, &CancellationToken::new())
    }

    pub fn load_table_controlled(
        self: &Arc<Self>,
        table: &str,
        max_rows: u64,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, String> {
        if max_rows == 0 || max_rows > ROW_CAP {
            return Err(format!("Row limit must be between 1 and {ROW_CAP}."));
        }
        let session = self.session.as_ref().ok_or("the session is closed")?;
        let (columns, rows, scan, truncated) =
            self.runtime
                .block_on(read_pages(session, table, max_rows, cancellation))?;
        Ok(LiveTable {
            session: self.clone(),
            columns,
            rows,
            scan,
            truncated,
            table: table.to_string(),
        })
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        let _inside = self.runtime.enter();
        drop(self.session.take());
    }
}

fn table_label(table: &TableInfo) -> String {
    match &table.schema {
        Some(schema) => format!("{schema}.{}", table.name),
        None => table.name.clone(),
    }
}

pub struct LiveTable {
    session: Arc<LiveSession>,
    columns: Vec<ColumnMeta>,
    rows: Vec<Vec<Cell>>,
    scan: ScanSource,
    pub truncated: bool,
    table: String,
}

impl LiveTable {
    pub fn session(&self) -> &Arc<LiveSession> {
        &self.session
    }

    pub fn table_name(&self) -> &str {
        &self.table
    }

    pub fn load(origin: &Origin, table: &str, max_rows: u64) -> Result<Self, String> {
        LiveSession::open(origin)?.load_table(table, max_rows)
    }
}

async fn open_session(origin: &Origin, cancellation: CancellationToken) -> Result<(ReadSession, String), String> {
    let registry = Arc::new(drivers_registry::default_registry());
    let (opened, label) = match origin {
        Origin::Saved(connection) => {
            let saved = list_saved_connections().await?;
            let summary = saved
                .iter()
                .find(|saved| saved.id.to_string() == *connection || saved.name == *connection)
                .ok_or_else(|| format!("no saved connection named or numbered {connection}"))?;
            (
                ReadSession::open_saved_controlled(summary.id, registry, cancellation).await,
                summary.name.clone(),
            )
        }
        Origin::File(path) => (
            ReadSession::open_file_controlled(path, registry, cancellation).await,
            path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
        ),
    };
    Ok((opened.map_err(|failure| format!("{failure:?}"))?, label))
}

type Pages = (Vec<ColumnMeta>, Vec<Vec<Cell>>, ScanSource, bool);

async fn read_pages(
    session: &ReadSession,
    table: &str,
    max_rows: u64,
    cancellation: &CancellationToken,
) -> Result<Pages, String> {
    let table = find_table(&session.tables, table)?;
    let spool = StreamResultSpoolManager::from_environment()
        .create()
        .map_err(|error| error.to_string())?;
    let mut infos: Vec<ColumnInfo> = Vec::new();
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut truncated = false;
    let mut page_index = 0;
    let mut budget = QueryResultBudget::default();
    while (rows.len() as u64) < max_rows {
        let limit = PAGE_ROWS.min(max_rows - rows.len() as u64);
        let control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
        let mut page = session
            .read_page(&table, rows.len() as u64, limit, &control)
            .await
            .map_err(|failure| format!("{failure:?}"))?;
        if infos.is_empty() {
            infos.clone_from(&page.columns);
        }
        if cancellation.is_cancelled() {
            return Err("The operation was cancelled.".into());
        }
        let incomplete = bound_page(&mut page, &mut budget);
        let count = page.rows.len() as u64;
        rows.extend(page.rows.iter().map(|row| row.to_vec()));
        spool
            .append_page(page_index, Arc::new(page))
            .await
            .map_err(|error| error.to_string())?;
        if cancellation.is_cancelled() {
            return Err("The operation was cancelled.".into());
        }
        page_index += 1;
        if incomplete {
            truncated = true;
            break;
        }
        if count < limit {
            break;
        }
        truncated = (rows.len() as u64) >= max_rows;
    }
    let total = rows.len() as u64;
    let columns = infos.iter().map(column_meta).collect();
    Ok((columns, rows, ScanSource::new(spool, infos, total), truncated))
}

async fn query_pages(
    session: &ReadSession,
    sql: &str,
    max_rows: u64,
    cancellation: &CancellationToken,
) -> Result<Pages, String> {
    let control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
    let mut result = session
        .run_query(sql, &control)
        .await
        .map_err(|failure| format!("{failure:?}"))?;
    if cancellation.is_cancelled() {
        return Err("The operation was cancelled.".into());
    }
    let spool = StreamResultSpoolManager::from_environment()
        .create()
        .map_err(|error| error.to_string())?;
    let infos: Vec<ColumnInfo> = result.columns.clone();
    let cap = usize::try_from(max_rows).unwrap_or(usize::MAX);
    let mut truncated = result.truncated || result.rows.len() > cap;
    let mut all: Vec<Vec<bookie_core::Value>> = result.rows.iter().take(cap).map(|row| row.to_vec()).collect();
    result.rows = bookie_core::QueryRows::from_rows(Vec::new());
    let mut budget = QueryResultBudget::default();
    let admitted = all.iter().take_while(|row| budget.admit(row)).count();
    if admitted < all.len() {
        all.truncate(admitted);
        truncated = true;
    }
    let rows: Vec<Vec<Cell>> = all.iter().map(|row| row.to_vec()).collect();
    for (index, chunk) in all.chunks(PAGE_ROWS as usize).enumerate() {
        let page = QueryResult {
            columns: infos.clone(),
            rows: bookie_core::QueryRows::from_rows(chunk.to_vec()),
            truncated: false,
        };
        spool
            .append_page(index as u64, Arc::new(page))
            .await
            .map_err(|error| error.to_string())?;
    }
    let total = all.len() as u64;
    let columns = infos.iter().map(column_meta).collect();
    Ok((columns, rows, ScanSource::new(spool, infos, total), truncated))
}

fn bound_page(page: &mut QueryResult, budget: &mut QueryResultBudget) -> bool {
    let admitted = page.rows.iter().take_while(|row| budget.admit(row)).count();
    let incomplete = page.truncated || admitted < page.rows.len();
    page.rows.truncate(admitted);
    page.truncated = incomplete;
    incomplete
}

fn find_table(tables: &[TableInfo], wanted: &str) -> Result<TableInfo, String> {
    tables
        .iter()
        .find(|table| {
            table.name == wanted
                || table
                    .schema
                    .as_ref()
                    .is_some_and(|schema| format!("{schema}.{}", table.name) == wanted)
        })
        .cloned()
        .ok_or_else(|| format!("no table named {wanted}"))
}

fn column_meta(info: &ColumnInfo) -> ColumnMeta {
    let width = (info.name.chars().count() as f32 * 9.0 + 48.0).clamp(110.0, 320.0);
    ColumnMeta {
        name: info.name.clone(),
        width,
        numeric: numeric_type(&info.data_type),
    }
}

fn numeric_type(data_type: &str) -> bool {
    let lower = data_type.to_ascii_lowercase();
    [
        "int", "serial", "numeric", "decimal", "number", "float", "double", "real", "money",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

impl ResultSource for LiveTable {
    fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }

    fn is_partial(&self) -> bool {
        self.truncated
    }

    fn origin(&self) -> String {
        let cap = if self.truncated {
            format!(" · {} rows loaded; result may be incomplete", self.rows.len())
        } else {
            String::new()
        };
        format!("{} · {} · read-only{cap}", self.session.driver_id, self.table)
    }

    fn total(&self) -> u64 {
        self.rows.len() as u64
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.rows.get(index))
            .cloned()
            .unwrap_or_default()
    }

    fn find(&self, query: &str, cap: usize, cancel: &CancellationToken) -> Result<FindResult, &'static str> {
        find_in(&self.scan, query, cap, cancel)
    }

    fn filter(&self, predicate: &Predicate, cancel: &CancellationToken) -> Result<Vec<u32>, &'static str> {
        filter_in(&self.scan, predicate, cancel)
    }

    fn compare(&self, _keyed: bool, _cancel: &CancellationToken) -> Result<Diff, &'static str> {
        Err("compare is not available for a live table")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_number_types_are_numeric_and_text_types_are_not() {
        for numeric in [
            "INTEGER",
            "bigint",
            "NUMERIC(10,2)",
            "double precision",
            "BIGSERIAL",
            "REAL",
            "decimal",
        ] {
            assert!(numeric_type(numeric), "{numeric}");
        }
        for other in ["TEXT", "varchar(20)", "timestamp", "uuid", "BLOB", ""] {
            assert!(!numeric_type(other), "{other}");
        }
    }

    struct FixtureFile(std::path::PathBuf);

    impl Drop for FixtureFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn oversized_fixture() -> FixtureFile {
        let path = std::env::temp_dir().join(format!("bookie-gpui-budget-{}.sqlite", uuid::Uuid::new_v4()));
        std::fs::File::create(&path).unwrap();
        let file = FixtureFile(path);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let registry = drivers_registry::default_registry();
            let driver = registry.get("sqlite").unwrap();
            let connection = driver
                .connect(bookie_core::ConnectOptions {
                    database: file.0.to_string_lossy().into_owned(),
                    ..Default::default()
                })
                .await
                .unwrap();
            connection.execute("CREATE TABLE wide (value BLOB)").await.unwrap();
            connection
                .execute("INSERT INTO wide VALUES (zeroblob(67108865))")
                .await
                .unwrap();
            connection.close().await.unwrap();
        });
        file
    }

    #[test]
    fn a_driver_truncated_page_is_reported_as_incomplete() {
        let file = oversized_fixture();
        let table = LiveTable::load(&Origin::File(file.0.clone()), "wide", 100).unwrap();
        assert_eq!(table.total(), 0);
        assert!(table.truncated);
    }

    #[test]
    fn an_empty_or_oversized_row_cap_is_refused() {
        let session = LiveSession::open(&fixture()).unwrap();
        assert!(session.load_table("orders", 0).is_err());
        assert!(session.load_table("orders", ROW_CAP + 1).is_err());
    }

    #[test]
    fn a_cancelled_live_load_is_not_returned_as_a_complete_table() {
        let session = LiveSession::open(&fixture()).unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert!(session.load_table_controlled("orders", 100, &cancellation).is_err());
    }

    #[test]
    fn decoded_budget_is_shared_across_pages_and_keeps_a_whole_row_prefix() {
        let make_page = || QueryResult {
            columns: Vec::new(),
            rows: bookie_core::QueryRows::from_rows(
                (0..40)
                    .map(|index| {
                        vec![
                            bookie_core::Value::Int(index),
                            bookie_core::Value::Text("x".repeat(1024 * 1024)),
                        ]
                    })
                    .collect(),
            ),
            truncated: false,
        };
        let mut budget = QueryResultBudget::default();
        let mut first = make_page();
        assert!(!bound_page(&mut first, &mut budget));
        assert_eq!(first.rows.len(), 40);
        drop(first);

        let mut second = make_page();
        assert!(bound_page(&mut second, &mut budget));
        assert!(second.truncated);
        assert!((1..40).contains(&second.rows.len()));
        for (index, row) in second.rows.iter().enumerate() {
            assert_eq!(row[0], bookie_core::Value::Int(index as i64));
        }
    }

    fn fixture() -> Origin {
        let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        Origin::File(std::path::PathBuf::from(manifest_dir).join("tests/fixtures/orders.sqlite"))
    }

    #[test]
    fn a_database_file_loads_as_a_live_table_with_real_columns_and_nulls() {
        let table = LiveTable::load(&fixture(), "orders", 100).expect("load");
        let names: Vec<&str> = table.columns().iter().map(|column| column.name.as_str()).collect();
        assert_eq!(names, ["id", "customer", "total", "paid", "note"]);
        assert_eq!(table.total(), 6);
        assert!(!table.truncated);
        assert_eq!(table.row(1)[1], Cell::Text("Grace".into()));
        assert_eq!(table.row(1)[4], Cell::Null);
        assert_eq!(table.row(2)[4], Cell::Text(String::new()));
    }

    #[test]
    fn find_and_filter_answer_from_the_loaded_rows() {
        let table = LiveTable::load(&fixture(), "orders", 100).expect("load");
        let cancel = CancellationToken::new();
        let found = table.find("ada", 10, &cancel).expect("find");
        assert_eq!(found.matches, vec![0, 3]);
        let columns = table.columns().to_vec();
        let predicate = crate::model::parse_predicate("customer = Ada", &columns).expect("predicate");
        assert_eq!(table.filter(&predicate, &cancel).expect("filter"), vec![0, 3]);
    }

    #[test]
    fn one_session_loads_each_of_its_tables() {
        let session = LiveSession::open(&fixture()).expect("open");
        assert_eq!(session.tables, ["customers", "orders"]);
        assert_eq!(session.label, "orders.sqlite");
        let customers = session.load_table("customers", 100).expect("customers");
        let orders = session.load_table("orders", 100).expect("orders");
        assert_eq!((customers.total(), orders.total()), (2, 6));
        assert_eq!(customers.columns().len(), 2);
        assert_eq!(orders.columns().len(), 5);
    }

    #[test]
    fn a_query_becomes_a_grid_source_with_its_own_columns_and_cap() {
        let session = LiveSession::open(&fixture()).expect("open");
        let result = session
            .run_query("SELECT customer, total FROM orders WHERE paid = 1 ORDER BY id", 100)
            .expect("query");
        let names: Vec<&str> = result.columns().iter().map(|column| column.name.as_str()).collect();
        assert_eq!(names, ["customer", "total"]);
        assert_eq!(result.total(), 3);
        assert_eq!(result.row(0)[0], Cell::Text("Ada".into()));
        let cancel = CancellationToken::new();
        assert_eq!(result.find("linus", 5, &cancel).expect("find").matches, vec![1]);
        let capped = session.run_query("SELECT id FROM orders", 2).expect("capped");
        assert_eq!((capped.total(), capped.truncated), (2, true));
        assert!(session.run_query("DELETE FROM orders", 100).is_err());
        assert!(session.run_query("SELECT nope FROM orders", 100).is_err());
    }

    #[test]
    fn the_row_cap_truncates_and_says_so() {
        let table = LiveTable::load(&fixture(), "orders", 4).expect("load");
        assert_eq!(table.total(), 4);
        assert!(table.truncated);
        assert_eq!(
            table.origin(),
            "sqlite · orders · read-only · 4 rows loaded; result may be incomplete"
        );
        let whole = LiveTable::load(&fixture(), "orders", 100).expect("load");
        assert_eq!(whole.origin(), "sqlite · orders · read-only");
    }

    #[test]
    fn a_missing_table_or_file_is_an_error_not_a_panic() {
        assert!(LiveTable::load(&fixture(), "nope", 10).is_err());
        let missing = Origin::File("/nonexistent/none.sqlite".into());
        assert!(LiveTable::load(&missing, "orders", 10).is_err());
    }
}
