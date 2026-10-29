use std::collections::VecDeque;
use std::sync::Arc;

use tablepro_app_services::stream_result_spool::StreamResultSpool;
use tablepro_core::{ColumnInfo, MAX_QUERY_RESULT_BYTES, QueryResult};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PageAppendError {
    OutOfOrder,
    SchemaChanged,
    InvalidRowWidth,
}

#[derive(Debug, Clone)]
pub enum StreamTerminal {
    Loading,
    Complete,
    Cancelled,
    TimedOut,
    Failed(super::StatementFailure),
}

#[derive(Debug, Clone)]
pub struct StreamedResult {
    pub(super) pages: Vec<Arc<QueryResult>>,
    pub(super) page_indices: Vec<u64>,
    pub(super) page_row_counts: Vec<usize>,
    pub(super) columns: Vec<ColumnInfo>,
    pub(super) total_pages: u64,
    pub(super) total_rows: usize,
    pub(super) terminal: StreamTerminal,
    pub(super) export: Option<crate::ui::export_dialog::StreamExportRequest>,
    pub(super) generation: u64,
    pub(super) spool: Option<StreamResultSpool>,
}

#[derive(Default)]
pub(super) struct StreamPageCache {
    pages: VecDeque<(u64, Arc<QueryResult>, usize)>,
    retained_bytes: usize,
    columns: Option<Vec<ColumnInfo>>,
    next_page: u64,
    empty_pages: u64,
    page_row_counts: Vec<usize>,
    total_rows: usize,
}

impl StreamPageCache {
    pub(super) fn append(&mut self, index: u64, result: Arc<QueryResult>) -> Result<(), PageAppendError> {
        if index != self.next_page {
            return Err(PageAppendError::OutOfOrder);
        }
        if result.rows.iter().any(|row| row.len() != result.columns.len()) {
            return Err(PageAppendError::InvalidRowWidth);
        }
        if self.columns.as_ref().is_some_and(|columns| *columns != result.columns) {
            return Err(PageAppendError::SchemaChanged);
        }
        if self.columns.is_none() {
            self.columns = Some(result.columns.clone());
        }
        self.page_row_counts.push(result.rows.len());
        if index > 0 && result.rows.is_empty() {
            self.next_page = self.next_page.saturating_add(1);
            self.empty_pages = self.empty_pages.saturating_add(1);
            return Ok(());
        }
        self.total_rows = self.total_rows.saturating_add(result.rows.len());
        let bytes = result.estimated_row_bytes();
        self.retained_bytes = self.retained_bytes.saturating_add(bytes);
        self.pages.push_back((index, result, bytes));
        self.next_page = self.next_page.saturating_add(1);
        while self.pages.len() > 1 && self.retained_bytes > MAX_QUERY_RESULT_BYTES {
            if let Some((_, _, bytes)) = self.pages.pop_front() {
                self.retained_bytes = self.retained_bytes.saturating_sub(bytes);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn pages(&self) -> &VecDeque<(u64, Arc<QueryResult>, usize)> {
        &self.pages
    }

    #[cfg(test)]
    pub(super) fn first_page(&self) -> u64 {
        self.pages.front().map_or(self.next_page, |(index, _, _)| *index)
    }

    pub(super) fn total_rows(&self) -> usize {
        self.total_rows
    }

    #[cfg(test)]
    pub(super) fn columns(&self) -> Option<&[ColumnInfo]> {
        self.columns.as_deref()
    }

    #[cfg(test)]
    pub(super) fn snapshot(
        &self,
        terminal: StreamTerminal,
        query: Option<crate::ui::export_dialog::StreamExportQuery>,
    ) -> StreamedResult {
        self.snapshot_with_spool(terminal, query, 0, None)
    }

    pub(super) fn snapshot_with_spool(
        &self,
        terminal: StreamTerminal,
        query: Option<crate::ui::export_dialog::StreamExportQuery>,
        generation: u64,
        spool: Option<StreamResultSpool>,
    ) -> StreamedResult {
        let export = query
            .zip(self.columns.clone())
            .map(|(query, columns)| crate::ui::export_dialog::StreamExportRequest { query, columns });
        StreamedResult {
            pages: self.pages.iter().map(|(_, page, _)| page.clone()).collect(),
            page_indices: self.pages.iter().map(|(index, _, _)| *index).collect(),
            page_row_counts: self.page_row_counts.clone(),
            columns: self.columns.clone().unwrap_or_default(),
            total_pages: self.next_page.saturating_sub(self.empty_pages),
            total_rows: self.total_rows,
            terminal,
            export,
            generation,
            spool,
        }
    }
}

#[cfg(test)]
#[path = "stream_pages_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stream_pages/performance_tests.rs"]
mod performance_tests;
