use bookie_core::Value;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::result_predicate::ColumnScope;
use crate::result_scan::{ScanProgress, ScanSource};
use crate::result_test_support::{column, numbered, source_of};

fn contains(needle: &str) -> ResultPredicate {
    ResultPredicate::Contains {
        scope: ColumnScope::Any,
        needle: needle.into(),
        options: Default::default(),
    }
}

fn filter(source: &ScanSource, predicate: ResultPredicate) -> Result<FilteredResult, FilterError> {
    filter_into_spool(
        source,
        predicate,
        &StreamResultSpoolManager::default(),
        &CancellationToken::new(),
        &ScanProgress::default(),
    )
}

fn derived_ids(result: &FilteredResult) -> Vec<i64> {
    let source = ScanSource::new(
        result.spool.clone(),
        vec![column("id"), column("label")],
        result.total_rows,
    );
    (0..source.page_count())
        .flat_map(|index| {
            let page = source.load_page(index).unwrap();
            page.rows
                .iter()
                .map(|row| match row[0] {
                    Value::Int(id) => id,
                    _ => -1,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn matching_rows_keep_source_order_in_a_new_spool() {
    let source = source_of(vec![numbered(0, 5), numbered(5, 5)]);
    let result = filter(&source, contains("row 3")).unwrap();
    assert_eq!(derived_ids(&result), vec![3]);
    assert_eq!(result.total_rows, 1);
    assert_eq!(result.rows_scanned, 10);
    let all = filter(&source, contains("row")).unwrap();
    assert_eq!(derived_ids(&all), (0..10).collect::<Vec<_>>());
}

#[test]
fn rows_gather_across_source_pages_and_empty_pages_are_not_written() {
    let source = source_of(vec![numbered(0, 3), numbered(3, 3), numbered(6, 3)]);
    let predicate = ResultPredicate::Equals {
        column: 0,
        value: Value::Int(7),
    };
    let result = filter(&source, predicate).unwrap();
    assert_eq!(result.page_row_counts, vec![1]);
    assert_eq!(result.spool.page_count(), 1);
}

#[test]
fn a_filter_with_no_matches_makes_an_empty_complete_result() {
    let source = source_of(vec![numbered(0, 3)]);
    let result = filter(&source, contains("absent")).unwrap();
    assert_eq!(result.total_rows, 0);
    assert!(result.page_row_counts.is_empty());
    assert_eq!(result.spool.page_count(), 0);
}

#[test]
fn output_pages_are_bounded_by_the_page_row_limit() {
    let pages = (0..3).map(|page| numbered(page * 5_000, 5_000)).collect();
    let source = source_of(pages);
    let result = filter(&source, contains("row")).unwrap();
    assert_eq!(result.page_row_counts, vec![5_000, 5_000, 5_000]);
}

#[test]
fn the_derived_spool_has_its_own_storage_and_releases_with_its_owner() {
    let manager = StreamResultSpoolManager::default();
    let source = source_of(vec![numbered(0, 50)]);
    let before = manager.reserved_bytes();
    let result = filter_into_spool(
        &source,
        contains("row"),
        &manager,
        &CancellationToken::new(),
        &ScanProgress::default(),
    )
    .unwrap();
    assert!(result.spool.stored_bytes() > 0);
    assert_eq!(manager.reserved_bytes(), before + result.spool.stored_bytes());
    drop(result);
    assert_eq!(manager.reserved_bytes(), before);
}

#[test]
fn a_quota_failure_drops_the_partial_result_and_keeps_the_source() {
    let source = source_of(vec![numbered(0, 200)]);
    let manager = StreamResultSpoolManager::with_limit(64);
    let error = filter_into_spool(
        &source,
        contains("row"),
        &manager,
        &CancellationToken::new(),
        &ScanProgress::default(),
    )
    .err()
    .unwrap();
    assert_eq!(error, FilterError::Spool(StreamResultSpoolError::QuotaExceeded));
    assert_eq!(manager.reserved_bytes(), 0);
    assert_eq!(source.load_page(0).unwrap().rows.len(), 200);
}

#[test]
fn a_cancelled_filter_leaves_no_reserved_storage() {
    let source = source_of(vec![numbered(0, 10)]);
    let manager = StreamResultSpoolManager::default();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = filter_into_spool(&source, contains("row"), &manager, &cancel, &ScanProgress::default())
        .err()
        .unwrap();
    assert_eq!(error, FilterError::Scan(ScanError::Cancelled));
    assert_eq!(manager.reserved_bytes(), 0);
}

#[test]
fn filtered_values_round_trip_exactly() {
    let rows = vec![
        vec![Value::Float(-0.0), Value::Text("keep".into())],
        vec![Value::Null, Value::Text("keep".into())],
        vec![Value::Float(1.0), Value::Text("drop".into())],
    ];
    let source = source_of(vec![rows]);
    let result = filter(&source, contains("keep")).unwrap();
    let derived = ScanSource::new(result.spool.clone(), vec![column("id"), column("label")], 2);
    let page = derived.load_page(0).unwrap();
    assert!(matches!(page.rows[0][0], Value::Float(value) if value.to_bits() == (-0.0_f64).to_bits()));
    assert_eq!(page.rows[1][0], Value::Null);
}
