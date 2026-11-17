use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::error::ExportError;
use super::paged::RowPage;
use crate::query::QueryResult;

pub fn limit_result(result: &QueryResult, max: usize) -> (QueryResult, bool) {
    let capped = result.rows.len() > max;
    let mut rows = result.rows.clone();
    rows.truncate(max);
    (
        QueryResult {
            columns: result.columns.clone(),
            rows,
            truncated: result.truncated,
        },
        capped,
    )
}

pub fn cap_pages(
    mut next_page: impl FnMut() -> Result<Option<RowPage>, ExportError>,
    max: usize,
    capped: Arc<AtomicBool>,
) -> impl FnMut() -> Result<Option<RowPage>, ExportError> {
    let mut taken = 0usize;
    let mut probed = false;
    move || {
        if taken >= max {
            if !probed {
                probed = true;
                if next_page()?.is_some_and(|page| !page.is_empty()) {
                    capped.store(true, Ordering::SeqCst);
                }
            }
            return Ok(None);
        }
        let Some(mut page) = next_page()? else {
            return Ok(None);
        };
        let room = max - taken;
        if page.len() > room {
            page.truncate(room);
            capped.store(true, Ordering::SeqCst);
            probed = true;
        }
        taken += page.len();
        Ok(Some(page))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::Value;

    fn pages(sizes: &[i64]) -> impl FnMut() -> Result<Option<RowPage>, ExportError> + use<> {
        let mut queue: std::collections::VecDeque<i64> = sizes.iter().copied().collect();
        let mut next = 0;
        move || {
            Ok(queue.pop_front().map(|size| {
                (0..size)
                    .map(|_| {
                        next += 1;
                        vec![Value::Int(next)]
                    })
                    .collect()
            }))
        }
    }

    fn drain(mut source: impl FnMut() -> Result<Option<RowPage>, ExportError>) -> usize {
        let mut rows = 0;
        while let Some(page) = source().unwrap() {
            rows += page.len();
        }
        rows
    }

    #[test]
    fn a_page_that_crosses_the_limit_is_cut_and_reported_capped() {
        let capped = Arc::new(AtomicBool::new(false));
        assert_eq!(drain(cap_pages(pages(&[4, 4]), 6, capped.clone())), 6);
        assert!(capped.load(Ordering::SeqCst));
    }

    #[test]
    fn a_source_that_ends_exactly_at_the_limit_is_not_capped() {
        let capped = Arc::new(AtomicBool::new(false));
        assert_eq!(drain(cap_pages(pages(&[3, 3]), 6, capped.clone())), 6);
        assert!(!capped.load(Ordering::SeqCst));
    }

    #[test]
    fn a_source_with_more_pages_after_the_limit_is_capped_without_reading_them_all() {
        let capped = Arc::new(AtomicBool::new(false));
        assert_eq!(drain(cap_pages(pages(&[3, 3, 3]), 3, capped.clone())), 3);
        assert!(capped.load(Ordering::SeqCst));
    }

    #[test]
    fn a_shorter_source_is_untouched() {
        let capped = Arc::new(AtomicBool::new(false));
        assert_eq!(drain(cap_pages(pages(&[2]), 10, capped.clone())), 2);
        assert!(!capped.load(Ordering::SeqCst));
    }

    #[test]
    fn a_loaded_result_is_cut_to_the_limit() {
        let result = QueryResult {
            columns: Vec::new(),
            rows: vec![vec![Value::Int(1)], vec![Value::Int(2)], vec![Value::Int(3)]].into(),
            truncated: false,
        };
        let (cut, capped) = limit_result(&result, 2);
        assert_eq!((cut.rows.len(), capped), (2, true));
        let (whole, capped) = limit_result(&result, 3);
        assert_eq!((whole.rows.len(), capped), (3, false));
    }
}
