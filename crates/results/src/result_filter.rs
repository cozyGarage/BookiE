use std::ops::ControlFlow;

use bookie_core::{MAX_QUERY_CURSOR_PAGE_ROWS, QueryResult, QueryResultBudget, QueryRows, Value};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::result_predicate::{CellTally, PredicateError, PreparedPredicate, ResultPredicate};
use crate::result_scan::{ScanError, ScanProgress, ScanSource, scan_pages};
use crate::stream_result_spool::{StreamResultSpool, StreamResultSpoolError, StreamResultSpoolManager};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FilterError {
    #[error(transparent)]
    Predicate(#[from] PredicateError),
    #[error(transparent)]
    Scan(ScanError),
    #[error(transparent)]
    Spool(#[from] StreamResultSpoolError),
}

impl From<ScanError> for FilterError {
    fn from(error: ScanError) -> Self {
        match error {
            ScanError::Spool(error) => Self::Spool(error),
            other => Self::Scan(other),
        }
    }
}

#[derive(Clone)]
pub struct FilteredResult {
    pub spool: StreamResultSpool,
    pub page_row_counts: Vec<usize>,
    pub total_rows: u64,
    pub rows_scanned: u64,
    pub unsearchable_cells: u64,
}

struct PageWriter {
    spool: StreamResultSpool,
    columns: Vec<bookie_core::ColumnInfo>,
    rows: QueryRows,
    budget: QueryResultBudget,
    page_row_counts: Vec<usize>,
}

impl PageWriter {
    fn push(&mut self, row: &[Value]) -> Result<(), FilterError> {
        if self.rows.len() == MAX_QUERY_CURSOR_PAGE_ROWS || !self.budget.admit(row) {
            self.flush()?;
            if !self.budget.admit(row) {
                return Err(FilterError::Scan(ScanError::TooManyRows));
            }
        }
        self.rows.push(row.to_vec());
        Ok(())
    }

    fn flush(&mut self) -> Result<(), FilterError> {
        if self.rows.is_empty() {
            return Ok(());
        }
        let rows = std::mem::take(&mut self.rows);
        let count = rows.len();
        let page = QueryResult {
            columns: self.columns.clone(),
            rows,
            truncated: false,
        };
        let index = self.page_row_counts.len() as u64;
        self.spool.append_page_sync(index, &page)?;
        self.page_row_counts.push(count);
        self.budget = QueryResultBudget::default();
        Ok(())
    }
}

pub fn filter_into_spool(
    source: &ScanSource,
    predicate: ResultPredicate,
    manager: &StreamResultSpoolManager,
    cancel: &CancellationToken,
    progress: &ScanProgress,
) -> Result<FilteredResult, FilterError> {
    let predicate = predicate.prepare(source.columns().len())?;
    let mut writer = PageWriter {
        spool: manager.create()?,
        columns: source.columns().to_vec(),
        rows: QueryRows::new(),
        budget: QueryResultBudget::default(),
        page_row_counts: Vec::new(),
    };
    let mut tally = CellTally::default();
    scan_pages(source, cancel, progress, |_, page| {
        copy_matches(page, &predicate, &mut writer, &mut tally)?;
        Ok(ControlFlow::Continue(()))
    })?;
    writer.flush()?;
    Ok(FilteredResult {
        total_rows: writer.page_row_counts.iter().map(|count| *count as u64).sum(),
        spool: writer.spool,
        page_row_counts: writer.page_row_counts,
        rows_scanned: progress.rows_done(),
        unsearchable_cells: tally.unsearchable_cells,
    })
}

fn copy_matches(
    page: &QueryResult,
    predicate: &PreparedPredicate,
    writer: &mut PageWriter,
    tally: &mut CellTally,
) -> Result<(), ScanError> {
    for row in page.rows.iter() {
        if predicate.matches(row, tally) {
            writer.push(row).map_err(filter_to_scan_error)?;
        }
    }
    Ok(())
}

fn filter_to_scan_error(error: FilterError) -> ScanError {
    match error {
        FilterError::Spool(error) => ScanError::Spool(error),
        FilterError::Scan(error) => error,
        FilterError::Predicate(_) => ScanError::Incomplete,
    }
}

#[cfg(test)]
mod tests;
