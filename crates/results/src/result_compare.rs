mod changes;
mod collector;
mod columns;
mod digest;
mod input;
mod keyed;
mod multiset;

use std::sync::Arc;

use bookie_core::{QueryResult, QueryRows, Value};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::result_scan::{ScanError, ScanProgress};
pub use changes::ChangedCells;
use columns::CommonColumns;
pub use input::CompareInput;

pub const MAX_COMPARE_ROWS: u64 = 1_000_000;
pub const MAX_COMPARE_DIFFERENCES: usize = 100_000;
pub const CHANGE_COLUMN: &str = "Change";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompareError {
    #[error("the comparison was cancelled")]
    Cancelled,
    #[error("a result is truncated or incomplete")]
    IncompleteInput,
    #[error("the results share no column names")]
    NoCommonColumns,
    #[error("a result has duplicate column names")]
    AmbiguousColumns,
    #[error("the key column is not in both results")]
    UnknownKeyColumn,
    #[error("a key value is NULL or could not be read")]
    UnreadableKey,
    #[error("a key value appears more than once")]
    DuplicateKey,
    #[error("a result has more rows than a comparison can read")]
    TooManyRows,
    #[error("the shared result memory budget is full")]
    BudgetExceeded,
    #[error("the saved result could not be read")]
    Source,
}

impl From<ScanError> for CompareError {
    fn from(error: ScanError) -> Self {
        match error {
            ScanError::Cancelled => Self::Cancelled,
            ScanError::Incomplete => Self::IncompleteInput,
            ScanError::TooManyRows => Self::TooManyRows,
            ScanError::BudgetExceeded => Self::BudgetExceeded,
            ScanError::Spool(_) => Self::Source,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMode {
    WholeRow,
    Keyed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareSummary {
    pub mode: CompareMode,
    pub key_columns: usize,
    pub rows_left: u64,
    pub rows_right: u64,
    pub equal: u64,
    pub added: u64,
    pub removed: u64,
    pub changed: u64,
    pub unknown_left: u64,
    pub unknown_right: u64,
    pub differences_capped: bool,
}

#[derive(Debug, Clone)]
pub struct CompareReport {
    pub summary: CompareSummary,
    pub diff: Arc<QueryResult>,
    pub changed_cells: Arc<ChangedCells>,
    pub columns_only_left: Vec<String>,
    pub columns_only_right: Vec<String>,
}

pub struct CompareRequest {
    pub left: CompareInput,
    pub right: CompareInput,
    pub key_columns: Vec<String>,
}

pub fn shared_column_names(
    left: &[bookie_core::ColumnInfo],
    right: &[bookie_core::ColumnInfo],
) -> Result<Vec<String>, CompareError> {
    CommonColumns::resolve(left, right, &[]).map(CommonColumns::into_names)
}

pub fn compare_results(
    request: &CompareRequest,
    cancel: &CancellationToken,
    progress: &ScanProgress,
) -> Result<CompareReport, CompareError> {
    let common = CommonColumns::resolve(request.left.columns(), request.right.columns(), &request.key_columns)?;
    let outcome = if request.key_columns.is_empty() {
        multiset::compare(request, &common, cancel, progress)?
    } else {
        keyed::compare(request, &common, cancel, progress)?
    };
    Ok(outcome.into_report(&common, request))
}

pub(crate) struct DiffRow {
    pub change: &'static str,
    pub values: Vec<Value>,
}

pub(crate) struct Outcome {
    pub summary: CompareSummary,
    pub rows: Vec<DiffRow>,
}

impl Outcome {
    fn into_report(self, common: &CommonColumns, request: &CompareRequest) -> CompareReport {
        let changed_cells = Arc::new(changes::changed_cells(&self.rows));
        let mut rows = QueryRows::new();
        for row in self.rows {
            let mut cells = Vec::with_capacity(row.values.len() + 1);
            cells.push(Value::Text(row.change.into()));
            cells.extend(row.values);
            rows.push(cells);
        }
        CompareReport {
            summary: self.summary,
            diff: Arc::new(QueryResult {
                columns: common.diff_columns(),
                rows,
                truncated: false,
            }),
            changed_cells,
            columns_only_left: common.only_left(request.left.columns()),
            columns_only_right: common.only_right(request.right.columns()),
        }
    }
}

#[cfg(test)]
mod tests;
