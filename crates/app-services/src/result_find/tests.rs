use bookie_core::Value;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::result_predicate::ColumnScope;
use crate::result_scan::ScanProgress;
use crate::result_test_support::{numbered, source_of};

fn contains(needle: &str) -> ResultPredicate {
    ResultPredicate::Contains {
        scope: ColumnScope::Any,
        needle: needle.into(),
    }
}

fn find(source: &ScanSource, predicate: ResultPredicate) -> Result<FindReport, FindError> {
    find_matches(source, predicate, &CancellationToken::new(), &ScanProgress::default())
}

#[test]
fn matches_report_the_page_and_row_that_hold_them() {
    let source = source_of(vec![numbered(0, 3), numbered(3, 3), numbered(6, 3)]);
    let report = find(&source, contains("row 4")).unwrap();
    assert_eq!(report.matches, vec![FindMatch { page: 1, row: 1 }]);
    assert!(!report.capped);
    assert_eq!(report.rows_scanned, 9);
}

#[test]
fn matches_keep_result_order_across_pages() {
    let source = source_of(vec![numbered(0, 3), numbered(3, 3)]);
    let report = find(&source, contains("row")).unwrap();
    let positions: Vec<(usize, usize)> = report.matches.iter().map(|hit| (hit.page, hit.row)).collect();
    assert_eq!(positions, vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)]);
}

#[test]
fn the_match_list_is_capped_and_the_scan_stops() {
    let pages = (0..3).map(|page| numbered(page * 5_000, 5_000)).collect();
    let source = source_of(pages);
    let progress = ScanProgress::default();
    let report = find_matches(&source, contains("row"), &CancellationToken::new(), &progress).unwrap();
    assert_eq!(report.matches.len(), MAX_FIND_MATCHES);
    assert!(report.capped);
    assert_eq!(progress.pages_done(), 3);
}

#[test]
fn a_typed_equality_finds_only_the_exact_value() {
    let source = source_of(vec![numbered(0, 4)]);
    let predicate = ResultPredicate::Equals {
        column: 0,
        value: Value::Int(2),
    };
    let report = find(&source, predicate).unwrap();
    assert_eq!(report.matches, vec![FindMatch { page: 0, row: 2 }]);
    let text_two = ResultPredicate::Equals {
        column: 0,
        value: Value::Text("2".into()),
    };
    assert!(find(&source, text_two).unwrap().matches.is_empty());
}

#[test]
fn unsearchable_cells_are_counted_not_matched() {
    let rows = vec![
        vec![Value::Int(1), Value::Undecodable("numeric".into())],
        vec![Value::Int(2), Value::Text("hit".into())],
    ];
    let source = source_of(vec![rows]);
    let report = find(&source, contains("hit")).unwrap();
    assert_eq!(report.matches.len(), 1);
    assert_eq!(report.unsearchable_cells, 1);
}

#[test]
fn an_invalid_predicate_is_refused_before_any_page_loads() {
    let source = source_of(vec![numbered(0, 2)]);
    let error = find(&source, contains("")).unwrap_err();
    assert_eq!(error, FindError::Predicate(PredicateError::EmptyNeedle));
}

#[test]
fn a_cancelled_find_returns_cancelled() {
    let source = source_of(vec![numbered(0, 2)]);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = find_matches(&source, contains("row"), &cancel, &ScanProgress::default()).unwrap_err();
    assert_eq!(error, FindError::Scan(ScanError::Cancelled));
}
