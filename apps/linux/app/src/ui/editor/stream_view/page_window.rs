use std::collections::{HashSet, VecDeque};

pub(super) struct PageWindow<T> {
    wanted: Vec<u64>,
    pending: HashSet<u64>,
    failed: HashSet<u64>,
    deferred: HashSet<u64>,
    pages: VecDeque<(u64, T, usize)>,
    bytes: usize,
    byte_limit: usize,
    concurrent_limit: usize,
}

impl<T> PageWindow<T> {
    pub(super) fn new(byte_limit: usize, concurrent_limit: usize) -> Self {
        Self {
            wanted: Vec::new(),
            pending: HashSet::new(),
            failed: HashSet::new(),
            deferred: HashSet::new(),
            pages: VecDeque::new(),
            bytes: 0,
            byte_limit,
            concurrent_limit,
        }
    }

    pub(super) fn show(&mut self, wanted: &[u64]) {
        if self.wanted != wanted {
            self.wanted = wanted.to_vec();
            self.deferred.retain(|page| wanted.contains(page));
            self.failed.retain(|page| wanted.contains(page));
        }
    }

    pub(super) fn next_request(&mut self) -> Option<u64> {
        if self.pending.len() >= self.concurrent_limit {
            return None;
        }
        let page = self.wanted.iter().copied().find(|page| {
            !self.pending.contains(page)
                && !self.failed.contains(page)
                && !self.deferred.contains(page)
                && !self.pages.iter().any(|(index, _, _)| index == page)
        })?;
        self.pending.insert(page);
        Some(page)
    }

    pub(super) fn loaded(&mut self, page: u64, data: T, bytes: usize) -> Option<Vec<u64>> {
        let mut evictions = Vec::new();
        if !self.pending.remove(&page) || !self.wanted.contains(&page) {
            return None;
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.pages.push_back((page, data, bytes));
        while self.bytes > self.byte_limit && self.pages.len() > 1 {
            let position = self
                .pages
                .iter()
                .position(|(index, _, _)| !self.wanted.contains(index))
                .unwrap_or(0);
            if let Some((evicted, _, size)) = self.pages.remove(position) {
                self.bytes = self.bytes.saturating_sub(size);
                if self.wanted.contains(&evicted) {
                    self.deferred.insert(evicted);
                }
                evictions.push(evicted);
            }
        }
        Some(evictions)
    }

    pub(super) fn failed(&mut self, page: u64) {
        if self.pending.remove(&page) && self.wanted.contains(&page) {
            self.failed.insert(page);
        }
    }

    pub(super) fn retry(&mut self, page: u64) {
        self.failed.remove(&page);
        self.deferred.remove(&page);
    }

    pub(super) fn is_failed(&self, page: u64) -> bool {
        self.failed.contains(&page)
    }

    pub(super) fn get(&self, page: u64) -> Option<&T> {
        self.pages
            .iter()
            .find(|(index, _, _)| *index == page)
            .map(|(_, data, _)| data)
    }

    #[cfg(test)]
    fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_visible_pages_once_and_bounds_concurrent_loads() {
        let mut window = PageWindow::new(100, 2);
        window.show(&[1, 2, 3]);
        assert_eq!(window.next_request(), Some(1));
        assert_eq!(window.next_request(), Some(2));
        assert_eq!(window.next_request(), None);
        window.loaded(1, "one", 40);
        assert_eq!(window.next_request(), Some(3));
        assert_eq!(window.get(1), Some(&"one"));
    }

    #[test]
    fn discards_late_results_and_retries_only_after_explicit_request() {
        let mut window = PageWindow::new(100, 1);
        window.show(&[1]);
        assert_eq!(window.next_request(), Some(1));
        window.show(&[2]);
        window.loaded(1, "old", 20);
        assert_eq!(window.get(1), None);
        assert_eq!(window.next_request(), Some(2));
        window.failed(2);
        assert_eq!(window.next_request(), None);
        assert!(window.is_failed(2));
        window.retry(2);
        assert_eq!(window.next_request(), Some(2));
    }

    #[test]
    fn bounds_loaded_bytes_and_keeps_one_oversized_page() {
        let mut window = PageWindow::new(100, 2);
        window.show(&[1, 2]);
        assert_eq!(window.next_request(), Some(1));
        assert_eq!(window.next_request(), Some(2));
        window.loaded(1, "first", 80);
        window.loaded(2, "second", 80);
        assert!(window.bytes() <= 100);
        assert_eq!(window.get(2), Some(&"second"));
        assert_eq!(window.next_request(), None);
        window.show(&[3]);
        assert_eq!(window.next_request(), Some(3));
        window.loaded(3, "wide", 120);
        assert_eq!(window.get(3), Some(&"wide"));
        assert_eq!(window.bytes(), 120);
    }

    #[test]
    fn evicts_offscreen_pages_before_pages_in_the_viewport() {
        let mut window = PageWindow::new(100, 2);
        window.show(&[1]);
        assert_eq!(window.next_request(), Some(1));
        window.loaded(1, "visible", 40);
        window.show(&[2]);
        assert_eq!(window.next_request(), Some(2));
        window.loaded(2, "offscreen", 40);
        window.show(&[1, 3]);
        assert_eq!(window.next_request(), Some(3));
        window.loaded(3, "new", 40);
        assert_eq!(window.get(1), Some(&"visible"));
        assert_eq!(window.get(2), None);
        assert_eq!(window.get(3), Some(&"new"));
    }

    #[test]
    fn viewport_changes_do_not_reload_deferred_pages_until_they_leave_and_return() {
        let mut window = PageWindow::new(100, 2);
        window.show(&[1, 2]);
        assert_eq!(window.next_request(), Some(1));
        assert_eq!(window.next_request(), Some(2));
        window.loaded(1, "one", 80);
        window.loaded(2, "two", 80);
        window.show(&[1, 2, 3]);
        assert_eq!(window.next_request(), Some(3));
        window.loaded(3, "three", 80);
        window.show(&[2, 3]);
        window.show(&[1]);
        assert_eq!(window.next_request(), Some(1));
    }
}
