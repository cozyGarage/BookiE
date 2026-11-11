use std::ops::ControlFlow;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::result_test_support::{column, numbered, page, source_of};
use crate::stream_result_spool::StreamResultSpoolManager;

#[test]
fn a_scan_visits_every_page_in_order_and_reports_progress() {
    let source = source_of(vec![numbered(0, 3), numbered(3, 3), numbered(6, 1)]);
    let progress = ScanProgress::default();
    let mut seen = Vec::new();
    let end = scan_pages(&source, &CancellationToken::new(), &progress, |index, page| {
        seen.push((index, page.rows.len()));
        Ok(ControlFlow::Continue(()))
    })
    .unwrap();
    assert_eq!(end, ScanEnd::Finished);
    assert_eq!(seen, vec![(0, 3), (1, 3), (2, 1)]);
    assert_eq!((progress.pages_done(), progress.rows_done()), (3, 7));
}

#[test]
fn a_cancelled_scan_stops_before_the_next_page() {
    let source = source_of(vec![numbered(0, 2), numbered(2, 2), numbered(4, 2)]);
    let cancel = CancellationToken::new();
    let mut visited = 0;
    let error = scan_pages(&source, &cancel, &ScanProgress::default(), |_, _| {
        visited += 1;
        cancel.cancel();
        Ok(ControlFlow::Continue(()))
    })
    .unwrap_err();
    assert_eq!(error, ScanError::Cancelled);
    assert_eq!(visited, 1);
}

#[test]
fn cancellation_during_the_last_page_is_not_reported_as_complete() {
    let source = source_of(vec![numbered(0, 2)]);
    let cancel = CancellationToken::new();
    let error = scan_pages(&source, &cancel, &ScanProgress::default(), |_, _| {
        cancel.cancel();
        Ok(ControlFlow::Continue(()))
    })
    .unwrap_err();
    assert_eq!(error, ScanError::Cancelled);
}

#[test]
fn a_cancelled_token_prevents_any_page_from_loading() {
    let source = source_of(vec![numbered(0, 2)]);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = scan_pages(&source, &cancel, &ScanProgress::default(), |_, _| panic!("visited")).unwrap_err();
    assert_eq!(error, ScanError::Cancelled);
}

#[test]
fn breaking_stops_the_scan_without_an_integrity_error() {
    let source = source_of(vec![numbered(0, 2), numbered(2, 2)]);
    let end = scan_pages(&source, &CancellationToken::new(), &ScanProgress::default(), |_, _| {
        Ok(ControlFlow::Break(()))
    })
    .unwrap();
    assert_eq!(end, ScanEnd::Stopped);
}

#[test]
fn a_row_count_that_differs_from_the_result_is_incomplete() {
    let spool = StreamResultSpoolManager::default().create().unwrap();
    spool.append_page_sync(0, &page(numbered(0, 2))).unwrap();
    let source = ScanSource::new(spool, vec![column("id"), column("label")], 3);
    let error = scan_pages(&source, &CancellationToken::new(), &ScanProgress::default(), |_, _| {
        Ok(ControlFlow::Continue(()))
    })
    .unwrap_err();
    assert_eq!(error, ScanError::Incomplete);
}

#[test]
fn an_empty_result_with_no_pages_is_complete() {
    let source = source_of(Vec::new());
    let end = scan_pages(&source, &CancellationToken::new(), &ScanProgress::default(), |_, _| {
        Ok(ControlFlow::Continue(()))
    })
    .unwrap();
    assert_eq!(end, ScanEnd::Finished);
}

#[test]
fn a_visitor_error_ends_the_scan_with_that_error() {
    let source = source_of(vec![numbered(0, 2)]);
    let error = scan_pages(&source, &CancellationToken::new(), &ScanProgress::default(), |_, _| {
        Err(ScanError::TooManyRows)
    })
    .unwrap_err();
    assert_eq!(error, ScanError::TooManyRows);
}

#[test]
fn spool_stored_bytes_are_reported_and_shared_with_the_manager() {
    let manager = StreamResultSpoolManager::default();
    let spool = manager.create().unwrap();
    spool.append_page_sync(0, &page(numbered(0, 50))).unwrap();
    assert!(spool.stored_bytes() > 0);
    assert_eq!(manager.reserved_bytes(), spool.stored_bytes());
    drop(spool);
    assert_eq!(manager.reserved_bytes(), 0);
}

#[test]
fn a_scan_is_refused_when_the_shared_page_budget_is_exhausted() {
    let source = source_of(vec![numbered(0, 2), numbered(2, 2)]);
    let budget = source.spool().page_budget();
    let hold = budget.reserve(budget.limit_bytes()).unwrap();

    let error = scan_pages(&source, &CancellationToken::new(), &ScanProgress::default(), |_, _| {
        panic!("a refused scan must not decode a page")
    })
    .unwrap_err();

    assert_eq!(error, ScanError::PageBudget);
    drop(hold);
    assert_eq!(budget.used_bytes(), 0);
    let end = scan_pages(&source, &CancellationToken::new(), &ScanProgress::default(), |_, _| {
        Ok(ControlFlow::Continue(()))
    })
    .unwrap();
    assert_eq!(end, ScanEnd::Finished);
}

#[test]
fn a_page_is_held_in_the_budget_only_while_it_is_visited() {
    let source = source_of(vec![numbered(0, 3), numbered(3, 3)]);
    let budget = source.spool().page_budget();
    let mut held = Vec::new();

    scan_pages(
        &source,
        &CancellationToken::new(),
        &ScanProgress::default(),
        |_, page| {
            held.push((budget.used_bytes(), page.estimated_row_bytes(), budget.used_pages()));
            Ok(ControlFlow::Continue(()))
        },
    )
    .unwrap();

    assert_eq!(held.len(), 2);
    for (used, estimate, pages) in held {
        assert_eq!(used, estimate, "the reservation is resized to the decoded page");
        assert_eq!(pages, 1);
    }
    assert_eq!(budget.used_bytes(), 0);
    assert_eq!(budget.used_pages(), 0);
}

#[test]
fn stop_cancellation_and_visitor_failure_release_the_reservation() {
    let source = source_of(vec![numbered(0, 2), numbered(2, 2)]);
    let budget = source.spool().page_budget();

    scan_pages(&source, &CancellationToken::new(), &ScanProgress::default(), |_, _| {
        Ok(ControlFlow::Break(()))
    })
    .unwrap();
    assert_eq!(budget.used_bytes(), 0, "stop");

    let cancel = CancellationToken::new();
    let _ = scan_pages(&source, &cancel, &ScanProgress::default(), |_, _| {
        cancel.cancel();
        Ok(ControlFlow::Continue(()))
    });
    assert_eq!(budget.used_bytes(), 0, "cancellation");

    let _ = scan_pages(&source, &CancellationToken::new(), &ScanProgress::default(), |_, _| {
        Err(ScanError::TooManyRows)
    });
    assert_eq!(budget.used_bytes(), 0, "visitor failure");
}

#[test]
fn a_comparison_names_the_page_memory_refusal_instead_of_a_source_failure() {
    use crate::result_compare::CompareError;
    assert_eq!(CompareError::from(ScanError::PageBudget), CompareError::PageMemoryBusy);
}
