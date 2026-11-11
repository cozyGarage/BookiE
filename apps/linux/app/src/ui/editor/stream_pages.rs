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

#[derive(Clone)]
pub(in crate::ui) struct ResidentPageStore(
    Arc<Mutex<HashMap<u64, ResidentPage>>>,
    bookie_results::stream_page_budget::StreamPageBudget,
);

struct ResidentPage {
    result: Arc<QueryResult>,
    lease: bookie_results::stream_page_budget::StreamPageLease,
    cached: bool,
    visible: usize,
}

impl std::fmt::Debug for ResidentPageStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ResidentPageStore")
    }
}

impl Default for ResidentPageStore {
    fn default() -> Self {
        Self::with_budget(bookie_results::stream_page_budget::StreamPageBudget::default())
    }
}

impl ResidentPageStore {
    fn with_budget(budget: bookie_results::stream_page_budget::StreamPageBudget) -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())), budget)
    }

    #[cfg(test)]
    pub(super) fn from_pages(pages: impl IntoIterator<Item = (u64, Arc<QueryResult>)>) -> Self {
        let budget = bookie_results::stream_page_budget::StreamPageBudget::default();
        Self(
            Arc::new(Mutex::new(
                pages
                    .into_iter()
                    .filter_map(|(index, page)| {
                        budget.retain(&page).map(|lease| {
                            (
                                index,
                                ResidentPage {
                                    result: page,
                                    lease,
                                    cached: true,
                                    visible: 0,
                                },
                            )
                        })
                    })
                    .collect(),
            )),
            budget,
        )
    }

    pub(super) fn insert(
        &self,
        index: u64,
        page: Arc<QueryResult>,
        lease: bookie_results::stream_page_budget::StreamPageLease,
        cached: bool,
    ) -> Arc<QueryResult> {
        let mut pages = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = pages.get_mut(&index) {
            existing.cached |= cached;
            return existing.result.clone();
        }
        pages.insert(
            index,
            ResidentPage {
                result: page.clone(),
                lease,
                cached,
                visible: 0,
            },
        );
        page
    }

    fn remove(&self, index: u64) {
        let mut pages = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(page) = pages.get_mut(&index) {
            page.cached = false;
            if page.visible == 0 {
                pages.remove(&index);
            }
        }
    }

    pub(super) fn get(&self, index: u64) -> Option<Arc<QueryResult>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&index)
            .map(|page| page.result.clone())
    }

    pub(super) fn get_with_lease(
        &self,
        index: u64,
    ) -> Option<(Arc<QueryResult>, bookie_results::stream_page_budget::StreamPageLease)> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&index)
            .map(|page| (page.result.clone(), page.lease.clone()))
    }

    pub(super) fn protect(&self, index: u64) {
        if let Some(page) = self.0.lock().unwrap_or_else(PoisonError::into_inner).get_mut(&index) {
            page.visible = page.visible.saturating_add(1);
        }
    }

    pub(super) fn unprotect(&self, index: u64) {
        let mut pages = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let remove = pages.get_mut(&index).is_some_and(|page| {
            page.visible = page.visible.saturating_sub(1);
            page.visible == 0 && !page.cached
        });
        if remove {
            pages.remove(&index);
        }
    }

    pub(super) fn reserve(&self, bytes: usize) -> Option<bookie_results::stream_page_budget::StreamPageLease> {
        self.1.reserve(bytes)
    }

    pub(super) fn retain(
        &self,
        page: &Arc<QueryResult>,
    ) -> Option<bookie_results::stream_page_budget::StreamPageLease> {
        self.1.retain(page)
    }
}

pub(super) struct StreamPageCache {
    pages: VecDeque<(
        u64,
        Arc<QueryResult>,
        usize,
        bookie_results::stream_page_budget::StreamPageLease,
    )>,
    retained_bytes: usize,
    columns: Option<Vec<ColumnInfo>>,
    index: bookie_app_services::stream_page_validation::StreamPageIndex,
    retained_rows: usize,
    resident_pages: ResidentPageStore,
    unspooled_page: Option<u64>,
    #[cfg(test)]
    page_budget: bookie_results::stream_page_budget::StreamPageBudget,
}

impl Default for StreamPageCache {
    fn default() -> Self {
        Self::with_budget(bookie_results::stream_page_budget::StreamPageBudget::default())
    }
}

impl StreamPageCache {
    pub(super) fn with_budget(page_budget: bookie_results::stream_page_budget::StreamPageBudget) -> Self {
        Self {
            pages: VecDeque::new(),
            retained_bytes: 0,
            columns: None,
            index: Default::default(),
            retained_rows: 0,
            resident_pages: ResidentPageStore::with_budget(page_budget.clone()),
            unspooled_page: None,
            #[cfg(test)]
            page_budget,
        }
    }

    #[cfg(test)]
    pub(super) fn append(&mut self, index: u64, result: Arc<QueryResult>) -> Result<(), PageAppendError> {
        self.append_with_storage_status(index, result, true)
    }

    #[cfg(test)]
    pub(super) fn append_with_storage_status(
        &mut self,
        index: u64,
        result: Arc<QueryResult>,
        spooled: bool,
    ) -> Result<(), PageAppendError> {
        let lease = self.page_budget.retain(&result);
        self.append_with_optional_lease(index, result, spooled, lease)
    }

    pub(super) fn append_with_lease(
        &mut self,
        index: u64,
        result: Arc<QueryResult>,
        spooled: bool,
        lease: bookie_results::stream_page_budget::StreamPageLease,
    ) -> Result<(), PageAppendError> {
        self.append_with_optional_lease(index, result, spooled, Some(lease))
    }

    fn append_with_optional_lease(
        &mut self,
        index: u64,
        result: Arc<QueryResult>,
        spooled: bool,
        lease: Option<bookie_results::stream_page_budget::StreamPageLease>,
    ) -> Result<(), PageAppendError> {
        if lease.is_none() && !spooled {
            bookie_app_services::stream_page_validation::validate_page(
                index,
                self.index.next_page(),
                self.columns.as_deref(),
                &result,
            )?;
            return Err(PageAppendError::RetentionLimit);
        }
        let empty = self.index.accept(index, self.columns.as_deref(), &result)?;
        if self.columns.is_none() {
            self.columns = Some(result.columns.clone());
        }
        if empty {
            return Ok(());
        }
        let bytes = result.estimated_row_bytes();
        let Some(lease) = lease else {
            return Ok(());
        };
        self.retained_rows = self.retained_rows.saturating_add(result.rows.len());
        self.retained_bytes = self.retained_bytes.saturating_add(bytes);
        self.resident_pages.insert(index, result.clone(), lease.clone(), true);
        self.pages.push_back((index, result, bytes, lease));
        while self.pages.len() > 1 && self.retained_bytes > MAX_QUERY_RESULT_BYTES {
            if let Some((retired_index, retired, bytes, _lease)) = self.pages.pop_front() {
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
    pub(super) fn pages(
        &self,
    ) -> &VecDeque<(
        u64,
        Arc<QueryResult>,
        usize,
        bookie_results::stream_page_budget::StreamPageLease,
    )> {
        &self.pages
    }

    #[cfg(test)]
    pub(super) fn first_page(&self) -> u64 {
        self.pages
            .front()
            .map_or(self.index.next_page(), |(index, _, _, _)| *index)
    }

    pub(super) fn total_rows(&self) -> usize {
        self.index.total_rows()
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
                    total_rows: self.index.total_rows(),
                })
        } else {
            None
        };
        StreamedResult {
            pages: self.pages.iter().map(|(_, page, _, _)| Arc::downgrade(page)).collect(),
            page_indices: self.pages.iter().map(|(index, _, _, _)| *index).collect(),
            page_row_counts: self.index.row_counts().to_vec(),
            page_row_starts: self.index.row_starts().to_vec(),
            retained_rows: self.retained_rows,
            columns: self.columns.clone().unwrap_or_default(),
            total_pages: self.index.total_pages(),
            total_rows: self.index.total_rows(),
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
        while let Some((index, page, bytes, lease)) = self.pages.pop_front() {
            if Some(index) == keep_index {
                retained_bytes = bytes;
                retained_rows = page.rows.len();
                retained.push_back((index, page, bytes, lease));
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
