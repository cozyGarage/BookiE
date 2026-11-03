use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bookie_core::{ColumnInfo, QueryResult};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::stream_result_spool::{StreamResultSpool, StreamResultSpoolError};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScanError {
    #[error("the scan was cancelled")]
    Cancelled,
    #[error("the saved result is incomplete")]
    Incomplete,
    #[error("the scan found more rows than it can keep")]
    TooManyRows,
    #[error(transparent)]
    Spool(#[from] StreamResultSpoolError),
}

#[derive(Debug, Clone, Default)]
pub struct ScanProgress {
    pages: Arc<AtomicU64>,
    rows: Arc<AtomicU64>,
}

impl ScanProgress {
    pub fn pages_done(&self) -> u64 {
        self.pages.load(Ordering::Acquire)
    }

    pub fn rows_done(&self) -> u64 {
        self.rows.load(Ordering::Acquire)
    }

    fn advance(&self, rows: usize) {
        self.pages.fetch_add(1, Ordering::AcqRel);
        self.rows.fetch_add(rows as u64, Ordering::AcqRel);
    }
}

#[derive(Clone)]
pub struct ScanSource {
    spool: StreamResultSpool,
    columns: Vec<ColumnInfo>,
    expected_rows: u64,
}

impl ScanSource {
    pub fn new(spool: StreamResultSpool, columns: Vec<ColumnInfo>, expected_rows: u64) -> Self {
        Self {
            spool,
            columns,
            expected_rows,
        }
    }

    pub fn spool(&self) -> &StreamResultSpool {
        &self.spool
    }

    pub fn columns(&self) -> &[ColumnInfo] {
        &self.columns
    }

    pub fn expected_rows(&self) -> u64 {
        self.expected_rows
    }

    pub fn page_count(&self) -> usize {
        self.spool.page_count()
    }

    pub fn load_page(&self, index: usize) -> Result<Arc<QueryResult>, ScanError> {
        let index = u64::try_from(index).map_err(|_| StreamResultSpoolError::MissingPage)?;
        Ok(self.spool.load_page_sync(index, self.columns.clone())?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanEnd {
    Finished,
    Stopped,
}

pub fn scan_pages(
    source: &ScanSource,
    cancel: &CancellationToken,
    progress: &ScanProgress,
    mut visit: impl FnMut(usize, &QueryResult) -> Result<ControlFlow<()>, ScanError>,
) -> Result<ScanEnd, ScanError> {
    let mut rows_seen = 0_u64;
    for index in 0..source.page_count() {
        if cancel.is_cancelled() {
            return Err(ScanError::Cancelled);
        }
        let page = source.load_page(index)?;
        rows_seen = rows_seen.saturating_add(page.rows.len() as u64);
        let flow = visit(index, &page)?;
        progress.advance(page.rows.len());
        if flow.is_break() {
            return Ok(ScanEnd::Stopped);
        }
    }
    if cancel.is_cancelled() {
        return Err(ScanError::Cancelled);
    }
    if rows_seen != source.expected_rows {
        return Err(ScanError::Incomplete);
    }
    Ok(ScanEnd::Finished)
}

#[cfg(test)]
mod tests;
