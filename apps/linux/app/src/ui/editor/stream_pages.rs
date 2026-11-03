use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use bookie_app_services::stream_result_spool::StreamResultSpool;
use bookie_core::{ColumnInfo, MAX_QUERY_RESULT_BYTES, QueryResult};

pub(super) use bookie_app_services::stream_page_validation::PageAppendError;

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
    pub(super) pages: Vec<Weak<QueryResult>>,
    pub(super) page_indices: Vec<u64>,
    pub(super) page_row_counts: Vec<usize>,
    pub(super) page_row_starts: Vec<usize>,
    pub(super) retained_rows: usize,
    pub(super) columns: Vec<ColumnInfo>,
    pub(super) total_pages: u64,
    pub(super) total_rows: usize,
    pub(super) terminal: StreamTerminal,
    pub(super) export: Option<crate::ui::export_dialog::StreamExportRequest>,
    pub(super) generation: u64,
    pub(super) spool: Option<StreamResultSpool>,
    pub(super) resident_pages: ResidentPageStore,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ResidentPageStore(Arc<Mutex<HashMap<u64, Arc<QueryResult>>>>);

impl ResidentPageStore {
    #[cfg(test)]
    pub(super) fn from_pages(pages: impl IntoIterator<Item = (u64, Arc<QueryResult>)>) -> Self {
        Self(Arc::new(Mutex::new(pages.into_iter().collect())))
    }

    fn insert(&self, index: u64, page: Arc<QueryResult>) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(index, page);
    }

    fn remove(&self, index: u64) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).remove(&index);
    }

    pub(super) fn get(&self, index: u64) -> Option<Arc<QueryResult>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&index)
            .cloned()
    }
}

#[derive(Default)]
pub(super) struct StreamPageCache {
    pages: VecDeque<(u64, Arc<QueryResult>, usize)>,
    retained_bytes: usize,
    columns: Option<Vec<ColumnInfo>>,
    next_page: u64,
    empty_pages: u64,
    page_row_counts: Vec<usize>,
    page_row_starts: Vec<usize>,
    retained_rows: usize,
    total_rows: usize,
    resident_pages: ResidentPageStore,
    unspooled_page: Option<u64>,
}

impl StreamPageCache {
    #[cfg(test)]
    pub(super) fn append(&mut self, index: u64, result: Arc<QueryResult>) -> Result<(), PageAppendError> {
        self.append_with_storage_status(index, result, true)
    }

    pub(super) fn append_with_storage_status(
        &mut self,
        index: u64,
        result: Arc<QueryResult>,
        spooled: bool,
    ) -> Result<(), PageAppendError> {
        bookie_app_services::stream_page_validation::validate_page(
            index,
            self.next_page,
            self.columns.as_deref(),
            &result,
        )?;
        if self.columns.is_none() {
            self.columns = Some(result.columns.clone());
        }
        self.page_row_counts.push(result.rows.len());
        self.page_row_starts.push(self.total_rows.saturating_add(1));
        if index > 0 && result.rows.is_empty() {
            self.next_page = self.next_page.saturating_add(1);
            self.empty_pages = self.empty_pages.saturating_add(1);
            return Ok(());
        }
        self.total_rows = self.total_rows.saturating_add(result.rows.len());
        self.retained_rows = self.retained_rows.saturating_add(result.rows.len());
        let bytes = result.estimated_row_bytes();
        self.retained_bytes = self.retained_bytes.saturating_add(bytes);
        self.resident_pages.insert(index, result.clone());
        self.pages.push_back((index, result, bytes));
        self.next_page = self.next_page.saturating_add(1);
        while self.pages.len() > 1 && self.retained_bytes > MAX_QUERY_RESULT_BYTES {
            if let Some((retired_index, retired, bytes)) = self.pages.pop_front() {
                self.resident_pages.remove(retired_index);
                self.retained_bytes = self.retained_bytes.saturating_sub(bytes);
                self.retained_rows = self.retained_rows.saturating_sub(retired.rows.len());
            }
        }
        if !spooled {
            self.unspooled_page = Some(index);
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
        &mut self,
        terminal: StreamTerminal,
        query: Option<crate::ui::export_dialog::StreamExportQuery>,
    ) -> StreamedResult {
        self.snapshot_with_spool(terminal, query, 0, None)
    }

    pub(super) fn snapshot_with_spool(
        &mut self,
        terminal: StreamTerminal,
        query: Option<crate::ui::export_dialog::StreamExportQuery>,
        generation: u64,
        spool: Option<StreamResultSpool>,
    ) -> StreamedResult {
        if !matches!(&terminal, StreamTerminal::Loading) && spool.is_some() {
            self.release_spooled_pages();
        }
        let export = if matches!(terminal, StreamTerminal::Complete) && spool.is_some() {
            query
                .zip(self.columns.clone())
                .map(|(query, columns)| crate::ui::export_dialog::StreamExportRequest {
                    query,
                    columns,
                    spool: spool.clone(),
                    total_rows: self.total_rows,
                })
        } else {
            None
        };
        StreamedResult {
            pages: self.pages.iter().map(|(_, page, _)| Arc::downgrade(page)).collect(),
            page_indices: self.pages.iter().map(|(index, _, _)| *index).collect(),
            page_row_counts: self.page_row_counts.clone(),
            page_row_starts: self.page_row_starts.clone(),
            retained_rows: self.retained_rows,
            columns: self.columns.clone().unwrap_or_default(),
            total_pages: self.next_page.saturating_sub(self.empty_pages),
            total_rows: self.total_rows,
            terminal,
            export,
            generation,
            spool,
            resident_pages: self.resident_pages.clone(),
        }
    }

    fn release_spooled_pages(&mut self) {
        let keep_index = self.unspooled_page;
        let mut retained = VecDeque::new();
        let mut retained_bytes = 0;
        let mut retained_rows = 0;
        while let Some((index, page, bytes)) = self.pages.pop_front() {
            if Some(index) == keep_index {
                retained_bytes = bytes;
                retained_rows = page.rows.len();
                retained.push_back((index, page, bytes));
            } else {
                self.resident_pages.remove(index);
            }
        }
        self.pages = retained;
        self.retained_bytes = retained_bytes;
        self.retained_rows = retained_rows;
    }
}

pub(super) fn page_row_starts_from_counts(page_row_counts: &[usize]) -> Vec<usize> {
    let mut total_rows = 0usize;
    page_row_counts
        .iter()
        .map(|count| {
            let start = total_rows.saturating_add(1);
            total_rows = total_rows.saturating_add(*count);
            start
        })
        .collect()
}

#[cfg(test)]
#[path = "stream_pages_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stream_pages/performance_tests.rs"]
mod performance_tests;
