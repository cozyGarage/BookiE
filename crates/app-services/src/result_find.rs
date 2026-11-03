use std::ops::ControlFlow;

use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::result_predicate::{CellTally, PredicateError, ResultPredicate};
use crate::result_scan::{ScanError, ScanProgress, ScanSource, scan_pages};

pub const MAX_FIND_MATCHES: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindMatch {
    pub page: usize,
    pub row: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindReport {
    pub matches: Vec<FindMatch>,
    pub capped: bool,
    pub rows_scanned: u64,
    pub unsearchable_cells: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FindError {
    #[error(transparent)]
    Predicate(#[from] PredicateError),
    #[error(transparent)]
    Scan(#[from] ScanError),
}

pub fn find_matches(
    source: &ScanSource,
    predicate: ResultPredicate,
    cancel: &CancellationToken,
    progress: &ScanProgress,
) -> Result<FindReport, FindError> {
    let predicate = predicate.prepare(source.columns().len())?;
    let mut report = FindReport {
        matches: Vec::new(),
        capped: false,
        rows_scanned: 0,
        unsearchable_cells: 0,
    };
    let mut tally = CellTally::default();
    scan_pages(source, cancel, progress, |index, page| {
        for (row_index, row) in page.rows.iter().enumerate() {
            if !predicate.matches(row, &mut tally) {
                continue;
            }
            if report.matches.len() == MAX_FIND_MATCHES {
                report.capped = true;
                return Ok(ControlFlow::Break(()));
            }
            report.matches.push(FindMatch {
                page: index,
                row: row_index,
            });
        }
        report.rows_scanned = report.rows_scanned.saturating_add(page.rows.len() as u64);
        Ok(ControlFlow::Continue(()))
    })?;
    report.unsearchable_cells = tally.unsearchable_cells;
    Ok(report)
}

#[cfg(test)]
mod tests;
