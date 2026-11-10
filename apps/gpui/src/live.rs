use std::sync::Arc;
use std::time::Duration;

use bookie_app_services::read_session::{ReadSession, list_saved_connections};
use bookie_core::{ColumnInfo, OperationControl, TableInfo};
use bookie_results::result_scan::ScanSource;
use bookie_results::stream_result_spool::StreamResultSpoolManager;
use tokio_util::sync::CancellationToken;

use crate::core_bridge::value_to_cell;
use crate::model::{Cell, ColumnMeta, Diff, FindResult, PAGE_ROWS, Predicate};
use crate::source::{ResultSource, filter_in, find_in};

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
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let (session, label) = runtime.block_on(open_session(origin))?;
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
        let session = self.session.as_ref().ok_or("the session is closed")?;
        let (columns, rows, scan, truncated) = self.runtime.block_on(query_pages(session, sql, max_rows))?;
        Ok(LiveTable {
            session: self.clone(),
            columns,
            rows,
            scan,
            truncated,
            table: "query".into(),
            row_cap: max_rows,
        })
    }

    pub fn load_table(self: &Arc<Self>, table: &str, max_rows: u64) -> Result<LiveTable, String> {
        let session = self.session.as_ref().ok_or("the session is closed")?;
        let (columns, rows, scan, truncated) = self.runtime.block_on(read_pages(session, table, max_rows))?;
        Ok(LiveTable {
            session: self.clone(),
            columns,
            rows,
            scan,
            truncated,
            table: table.to_string(),
            row_cap: max_rows,
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
    row_cap: u64,
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

async fn open_session(origin: &Origin) -> Result<(ReadSession, String), String> {
    let registry = Arc::new(drivers_registry::default_registry());
    let (opened, label) = match origin {
        Origin::Saved(connection) => {
            let saved = list_saved_connections().await?;
            let summary = saved
                .iter()
                .find(|saved| saved.id.to_string() == *connection || saved.name == *connection)
                .ok_or_else(|| format!("no saved connection named or numbered {connection}"))?;
            (
                ReadSession::open_saved(summary.id, registry).await,
                summary.name.clone(),
            )
        }
        Origin::File(path) => (
            ReadSession::open_file(path, registry).await,
            path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
        ),
    };
    Ok((opened.map_err(|failure| format!("{failure:?}"))?, label))
}

type Pages = (Vec<ColumnMeta>, Vec<Vec<Cell>>, ScanSource, bool);

async fn read_pages(session: &ReadSession, table: &str, max_rows: u64) -> Result<Pages, String> {
    let table = find_table(&session.tables, table)?;
    let spool = StreamResultSpoolManager::from_environment()
        .create()
        .map_err(|error| error.to_string())?;
    let mut infos: Vec<ColumnInfo> = Vec::new();
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut truncated = false;
    let mut page_index = 0;
    while (rows.len() as u64) < max_rows {
        let limit = PAGE_ROWS.min(max_rows - rows.len() as u64);
        let control = OperationControl::with_timeout(PAGE_TIMEOUT);
        let page = session
            .read_page(&table, rows.len() as u64, limit, &control)
            .await
            .map_err(|failure| format!("{failure:?}"))?;
        if infos.is_empty() {
            infos.clone_from(&page.columns);
        }
        let count = page.rows.len() as u64;
        rows.extend(
            page.rows
                .iter()
                .map(|row| row.iter().cloned().map(value_to_cell).collect()),
        );
        spool
            .append_page(page_index, Arc::new(page))
            .await
            .map_err(|error| error.to_string())?;
        page_index += 1;
        if count < limit {
            break;
        }
        truncated = (rows.len() as u64) >= max_rows;
    }
    let total = rows.len() as u64;
    let columns = infos.iter().map(column_meta).collect();
    Ok((columns, rows, ScanSource::new(spool, infos, total), truncated))
}

async fn query_pages(session: &ReadSession, sql: &str, max_rows: u64) -> Result<Pages, String> {
    let control = OperationControl::with_timeout(PAGE_TIMEOUT);
    let result = session
        .run_query(sql, &control)
        .await
        .map_err(|failure| format!("{failure:?}"))?;
    let spool = StreamResultSpoolManager::from_environment()
        .create()
        .map_err(|error| error.to_string())?;
    let infos: Vec<ColumnInfo> = result.columns.clone();
    let all: Vec<Vec<bookie_core::Value>> = result.rows.iter().map(|row| row.to_vec()).collect();
    let truncated = all.len() as u64 > max_rows;
    let kept = &all[..all.len().min(usize::try_from(max_rows).unwrap_or(usize::MAX))];
    let rows: Vec<Vec<Cell>> = kept
        .iter()
        .map(|row| row.iter().cloned().map(value_to_cell).collect())
        .collect();
    for (index, chunk) in kept.chunks(PAGE_ROWS as usize).enumerate() {
        let page = bookie_core::QueryResult {
            columns: infos.clone(),
            rows: bookie_core::QueryRows::from_rows(chunk.to_vec()),
            truncated: false,
        };
        spool
            .append_page(index as u64, Arc::new(page))
            .await
            .map_err(|error| error.to_string())?;
    }
    let columns = infos.iter().map(column_meta).collect();
    Ok((
        columns,
        rows,
        ScanSource::new(spool, infos, kept.len() as u64),
        truncated,
    ))
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
    }
}

impl ResultSource for LiveTable {
    fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }

    fn origin(&self) -> String {
        let cap = if self.truncated {
            format!(" · first {} rows only", self.row_cap)
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

    fn fixture() -> Origin {
        Origin::File(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orders.sqlite"))
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
        assert_eq!(table.origin(), "sqlite · orders · read-only · first 4 rows only");
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
