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

const PAGE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct LiveTable {
    runtime: tokio::runtime::Runtime,
    session: Option<ReadSession>,
    columns: Vec<ColumnMeta>,
    rows: Vec<Vec<Cell>>,
    scan: ScanSource,
    pub driver_id: String,
    pub truncated: bool,
}

impl LiveTable {
    pub fn load(origin: &Origin, table: &str, max_rows: u64) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let loaded = runtime.block_on(load_pages(origin, table, max_rows));
        let (session, columns, rows, scan, truncated) = loaded?;
        let driver_id = session.driver_id.clone();
        Ok(Self {
            runtime,
            session: Some(session),
            columns,
            rows,
            scan,
            driver_id,
            truncated,
        })
    }
}

type Loaded = (ReadSession, Vec<ColumnMeta>, Vec<Vec<Cell>>, ScanSource, bool);

async fn load_pages(origin: &Origin, table: &str, max_rows: u64) -> Result<Loaded, String> {
    let registry = Arc::new(drivers_registry::default_registry());
    let session = match origin {
        Origin::Saved(connection) => {
            let saved = list_saved_connections().await?;
            let summary = saved
                .iter()
                .find(|saved| saved.id.to_string() == *connection || saved.name == *connection)
                .ok_or_else(|| format!("no saved connection named or numbered {connection}"))?;
            ReadSession::open_saved(summary.id, registry).await
        }
        Origin::File(path) => ReadSession::open_file(path, registry).await,
    }
    .map_err(|failure| format!("{failure:?}"))?;
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
    Ok((session, columns, rows, ScanSource::new(spool, infos, total), truncated))
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

impl Drop for LiveTable {
    fn drop(&mut self) {
        let _inside = self.runtime.enter();
        drop(self.session.take());
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
    fn the_row_cap_truncates_and_says_so() {
        let table = LiveTable::load(&fixture(), "orders", 4).expect("load");
        assert_eq!(table.total(), 4);
        assert!(table.truncated);
    }

    #[test]
    fn a_missing_table_or_file_is_an_error_not_a_panic() {
        assert!(LiveTable::load(&fixture(), "nope", 10).is_err());
        let missing = Origin::File("/nonexistent/none.sqlite".into());
        assert!(LiveTable::load(&missing, "orders", 10).is_err());
    }
}
