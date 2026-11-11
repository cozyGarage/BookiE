use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use bookie_core::QueryResult;

pub const MAX_RETAINED_STREAM_PAGE_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone)]
pub struct StreamPageBudget(Arc<Mutex<BudgetState>>);

#[derive(Debug)]
struct BudgetState {
    limit: usize,
    used: usize,
    used_cells: usize,
    used_pages: usize,
    next_id: u64,
    leases: HashMap<u64, (usize, Weak<Lease>)>,
    pages: HashMap<usize, u64>,
}

#[derive(Debug)]
pub struct StreamPageLease(Arc<Lease>);

#[derive(Debug)]
struct Lease {
    budget: Weak<Mutex<BudgetState>>,
    id: u64,
    page: AtomicUsize,
    bytes: AtomicUsize,
    cells: AtomicUsize,
}

impl Default for StreamPageBudget {
    fn default() -> Self {
        Self::new(MAX_RETAINED_STREAM_PAGE_BYTES)
    }
}

impl StreamPageBudget {
    pub fn new(limit: usize) -> Self {
        Self(Arc::new(Mutex::new(BudgetState {
            limit,
            used: 0,
            used_cells: 0,
            used_pages: 0,
            next_id: 0,
            leases: HashMap::new(),
            pages: HashMap::new(),
        })))
    }

    pub fn retain(&self, page: &Arc<QueryResult>) -> Option<StreamPageLease> {
        let page_id = Arc::as_ptr(page) as usize;
        let bytes = page.estimated_row_bytes();
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(lease) = state.pages.get(&page_id).and_then(|id| state.leases.get(id))
            && let Some(lease) = lease.1.upgrade()
        {
            return Some(StreamPageLease(lease));
        }
        let cells = page.rows.iter().map(|row| row.len()).sum();
        insert_lease(&self.0, &mut state, Some(page_id), bytes, cells)
    }

    pub fn reserve(&self, bytes: usize) -> Option<StreamPageLease> {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        insert_lease(&self.0, &mut state, None, bytes, 0)
    }

    pub fn used_bytes(&self) -> usize {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).used
    }

    pub fn used_cells(&self) -> usize {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).used_cells
    }

    pub fn used_pages(&self) -> usize {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).used_pages
    }

    pub fn limit_bytes(&self) -> usize {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).limit
    }
}

impl StreamPageLease {
    pub fn resize_page(&mut self, page: &Arc<QueryResult>) -> bool {
        let bytes = page.estimated_row_bytes();
        let cells = page.rows.iter().map(|row| row.len()).sum();
        self.resize_payload(bytes, cells, Arc::as_ptr(page) as usize)
    }

    pub fn resize_detached_page(&mut self, page: &QueryResult) -> bool {
        let bytes = page.estimated_row_bytes();
        let cells = page.rows.iter().map(|row| row.len()).sum();
        self.resize_payload(bytes, cells, 1)
    }

    fn resize_payload(&mut self, bytes: usize, cells: usize, page: usize) -> bool {
        if Arc::strong_count(&self.0) != 1 {
            return false;
        }
        let Some(budget) = self.0.budget.upgrade() else {
            return false;
        };
        let mut state = budget.lock().unwrap_or_else(PoisonError::into_inner);
        let used_without_lease = state.used.saturating_sub(self.0.bytes.load(Ordering::Acquire));
        let cells_without_lease = state.used_cells.saturating_sub(self.0.cells.load(Ordering::Acquire));
        let previous_page = self.0.page.load(Ordering::Acquire);
        if used_without_lease.saturating_add(bytes) > state.limit {
            return false;
        }
        state.used = used_without_lease.saturating_add(bytes);
        state.used_cells = cells_without_lease.saturating_add(cells);
        if let Some((registered_bytes, _)) = state.leases.get_mut(&self.0.id) {
            *registered_bytes = bytes;
        }
        self.0.bytes.store(bytes, Ordering::Release);
        self.0.cells.store(cells, Ordering::Release);
        if previous_page == 0 {
            state.used_pages = state.used_pages.saturating_add(1);
        } else if previous_page > 1 && previous_page != page && state.pages.get(&previous_page) == Some(&self.0.id) {
            state.pages.remove(&previous_page);
        }
        if page > 1 {
            state.pages.insert(page, self.0.id);
        }
        self.0.page.store(page, Ordering::Release);
        true
    }
}

fn insert_lease(
    budget: &Arc<Mutex<BudgetState>>,
    state: &mut BudgetState,
    page: Option<usize>,
    bytes: usize,
    cells: usize,
) -> Option<StreamPageLease> {
    if state.used.saturating_add(bytes) > state.limit {
        return None;
    }
    let id = state.next_id;
    state.next_id = state.next_id.wrapping_add(1);
    let lease = Arc::new(Lease {
        budget: Arc::downgrade(budget),
        id,
        page: AtomicUsize::new(page.unwrap_or_default()),
        bytes: AtomicUsize::new(bytes),
        cells: AtomicUsize::new(cells),
    });
    state.used = state.used.saturating_add(bytes);
    state.used_cells = state.used_cells.saturating_add(cells);
    state.used_pages = state.used_pages.saturating_add(usize::from(page.is_some()));
    state.leases.insert(id, (bytes, Arc::downgrade(&lease)));
    if let Some(page) = page {
        state.pages.insert(page, id);
    }
    Some(StreamPageLease(lease))
}

impl Clone for StreamPageLease {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let Some(budget) = self.budget.upgrade() else { return };
        let mut state = budget.lock().unwrap_or_else(PoisonError::into_inner);
        state.used = state.used.saturating_sub(self.bytes.load(Ordering::Acquire));
        state.leases.remove(&self.id);
        state.used_cells = state.used_cells.saturating_sub(self.cells.load(Ordering::Acquire));
        let page = self.page.load(Ordering::Acquire);
        if page != 0 {
            state.used_pages = state.used_pages.saturating_sub(1);
        }
        if page != 0 && state.pages.get(&page) == Some(&self.id) {
            state.pages.remove(&page);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::{ColumnInfo, Value};

    #[test]
    fn counts_a_shared_page_once_until_its_last_owner_releases_it() {
        let page = page();
        let budget = StreamPageBudget::new(page.estimated_row_bytes());
        let first = budget.retain(&page).expect("first lease");
        let second = budget.retain(&page).expect("shared lease");
        assert_eq!(budget.used_bytes(), page.estimated_row_bytes());
        assert_eq!(budget.used_cells(), 1);
        assert_eq!(budget.used_pages(), 1);
        drop(first);
        assert_eq!(budget.used_bytes(), page.estimated_row_bytes());
        drop(second);
        assert_eq!(budget.used_bytes(), 0);
        assert_eq!(budget.used_cells(), 0);
        assert_eq!(budget.used_pages(), 0);
    }

    #[test]
    fn refuses_pages_and_reservations_over_the_shared_limit() {
        let budget = StreamPageBudget::new(64);
        let reservation = budget.reserve(64).expect("reservation");
        assert!(budget.reserve(1).is_none());
        assert_eq!(budget.used_bytes(), 64);
        drop(reservation);
        assert_eq!(budget.used_bytes(), 0);
    }

    #[test]
    fn reservation_shrinks_to_the_decoded_page_size() {
        let budget = StreamPageBudget::new(1024);
        let mut reservation = budget.reserve(1024).expect("maximum page reservation");
        let page = page();
        assert_eq!(budget.used_pages(), 0);
        assert!(reservation.resize_page(&page));
        let expected_bytes = page.estimated_row_bytes();
        assert_eq!(budget.used_bytes(), expected_bytes);
        assert_eq!(budget.used_cells(), 1);
        assert_eq!(budget.used_pages(), 1);
        drop(reservation);
        assert_eq!(budget.used_bytes(), 0);
        assert_eq!(budget.used_cells(), 0);
        assert_eq!(budget.used_pages(), 0);
    }

    #[test]
    fn detached_export_page_stays_charged_until_its_lease_drops() {
        let page = page();
        let budget = StreamPageBudget::new(page.estimated_row_bytes());
        let mut lease = budget.reserve(page.estimated_row_bytes()).expect("reservation");

        assert!(lease.resize_detached_page(&page));
        assert_eq!(budget.used_bytes(), page.estimated_row_bytes());
        assert_eq!(budget.used_cells(), 1);
        assert_eq!(budget.used_pages(), 1);
        drop(lease);
        assert_eq!(budget.used_bytes(), 0);
        assert_eq!(budget.used_cells(), 0);
        assert_eq!(budget.used_pages(), 0);
    }

    fn page() -> Arc<QueryResult> {
        Arc::new(QueryResult {
            columns: vec![ColumnInfo {
                name: "value".into(),
                data_type: "text".into(),
                nullable: true,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            }],
            rows: vec![vec![Value::Text("x".repeat(32))]].into(),
            truncated: false,
        })
    }
}
