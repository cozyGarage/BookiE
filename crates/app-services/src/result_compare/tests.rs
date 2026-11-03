use std::str::FromStr;
use std::sync::Arc;

use bookie_core::{QueryResult, Value};
use rust_decimal::Decimal;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::result_scan::ScanSource;
use crate::result_test_support::column;
use crate::stream_result_spool::StreamResultSpoolManager;

fn result(names: &[&str], rows: Vec<Vec<Value>>) -> Arc<QueryResult> {
    Arc::new(QueryResult {
        columns: names.iter().map(|name| column(name)).collect(),
        rows: rows.into(),
        truncated: false,
    })
}

fn pairs(rows: &[(i64, &str)]) -> Vec<Vec<Value>> {
    rows.iter()
        .map(|(id, label)| vec![Value::Int(*id), Value::Text((*label).into())])
        .collect()
}

fn pages(rows: &[(i64, &str)]) -> CompareInput {
    CompareInput::Pages(vec![result(&["id", "label"], pairs(rows))])
}

fn run(left: CompareInput, right: CompareInput, keys: &[&str]) -> Result<CompareReport, CompareError> {
    let request = CompareRequest {
        left,
        right,
        key_columns: keys.iter().map(|key| (*key).to_string()).collect(),
    };
    compare_results(&request, &CancellationToken::new(), &ScanProgress::default())
}

fn changes(report: &CompareReport) -> Vec<(String, Vec<Value>)> {
    report
        .diff
        .rows
        .iter()
        .map(|row| match &row[0] {
            Value::Text(change) => (change.clone(), row[1..].to_vec()),
            other => panic!("unexpected change cell {other:?}"),
        })
        .collect()
}

#[test]
fn identical_results_have_no_differences() {
    let rows = [(1, "a"), (2, "b")];
    let report = run(pages(&rows), pages(&rows), &[]).unwrap();
    assert_eq!(report.summary.equal, 2);
    assert_eq!(report.summary.mode, CompareMode::WholeRow);
    assert!(report.diff.rows.is_empty());
    assert!(!report.summary.differences_capped);
}

#[test]
fn whole_row_compare_ignores_row_order() {
    let report = run(pages(&[(1, "a"), (2, "b")]), pages(&[(2, "b"), (1, "a")]), &[]).unwrap();
    assert_eq!(report.summary.equal, 2);
    assert!(report.diff.rows.is_empty());
}

#[test]
fn whole_row_compare_reports_added_and_removed_rows() {
    let report = run(pages(&[(1, "a"), (2, "b")]), pages(&[(2, "b"), (3, "c")]), &[]).unwrap();
    assert_eq!(
        (report.summary.added, report.summary.removed, report.summary.equal),
        (1, 1, 1)
    );
    let found = changes(&report);
    assert!(found.contains(&("Removed".into(), pairs(&[(1, "a")]).remove(0))));
    assert!(found.contains(&("Added".into(), pairs(&[(3, "c")]).remove(0))));
}

#[test]
fn duplicate_rows_are_counted_by_multiplicity() {
    let report = run(pages(&[(1, "a"), (1, "a"), (1, "a")]), pages(&[(1, "a")]), &[]).unwrap();
    assert_eq!(
        (report.summary.equal, report.summary.removed, report.summary.added),
        (1, 2, 0)
    );
    assert_eq!(report.diff.rows.len(), 2);
}

#[test]
fn null_empty_text_and_empty_bytes_are_different_rows() {
    let left = CompareInput::Pages(vec![result(
        &["v"],
        vec![vec![Value::Null], vec![Value::Text(String::new())]],
    )]);
    let right = CompareInput::Pages(vec![result(
        &["v"],
        vec![vec![Value::Bytes(Vec::new())], vec![Value::Null]],
    )]);
    let report = run(left, right, &[]).unwrap();
    assert_eq!(
        (report.summary.equal, report.summary.added, report.summary.removed),
        (1, 1, 1)
    );
}

#[test]
fn signed_zero_nan_payloads_and_decimal_scale_are_differences() {
    let other_nan = f64::from_bits(f64::NAN.to_bits() ^ 1);
    let left = vec![
        vec![Value::Float(0.0)],
        vec![Value::Float(f64::NAN)],
        vec![Value::Decimal(Decimal::from_str("1.0").unwrap())],
    ];
    let right = vec![
        vec![Value::Float(-0.0)],
        vec![Value::Float(other_nan)],
        vec![Value::Decimal(Decimal::from_str("1.00").unwrap())],
    ];
    let report = run(
        CompareInput::Pages(vec![result(&["v"], left)]),
        CompareInput::Pages(vec![result(&["v"], right)]),
        &[],
    )
    .unwrap();
    assert_eq!(
        (report.summary.equal, report.summary.added, report.summary.removed),
        (0, 3, 3)
    );
}

#[test]
fn keyed_compare_reports_changed_added_and_removed_rows() {
    let left = pages(&[(1, "a"), (2, "b"), (3, "c")]);
    let right = pages(&[(2, "b"), (3, "changed"), (4, "d")]);
    let report = run(left, right, &["id"]).unwrap();
    let summary = &report.summary;
    assert_eq!(
        (summary.equal, summary.changed, summary.added, summary.removed),
        (1, 1, 1, 1)
    );
    assert_eq!(summary.mode, CompareMode::Keyed);
    let found = changes(&report);
    let labels: Vec<&str> = found.iter().map(|(label, _)| label.as_str()).collect();
    assert!(labels.contains(&"Before") && labels.contains(&"After"));
    let before = found.iter().position(|(label, _)| label == "Before").unwrap();
    assert_eq!(found[before].1, pairs(&[(3, "c")]).remove(0));
    assert_eq!(found[before + 1], ("After".into(), pairs(&[(3, "changed")]).remove(0)));
}

#[test]
fn keyed_compare_marks_only_the_cells_that_changed() {
    let left = pages(&[(1, "a"), (3, "c")]);
    let right = pages(&[(1, "a"), (3, "changed"), (4, "d")]);
    let report = run(left, right, &["id"]).unwrap();
    let found = changes(&report);
    let before = found.iter().position(|(label, _)| label == "Before").unwrap();
    for row in [before, before + 1] {
        assert!(report.changed_cells.contains(row, 2));
        assert!(!report.changed_cells.contains(row, 0));
        assert!(!report.changed_cells.contains(row, 1));
    }
    let added = found.iter().position(|(label, _)| label == "Added").unwrap();
    assert!(!report.changed_cells.contains(added, 2));
}

#[test]
fn whole_row_compare_marks_no_changed_cells() {
    let report = run(pages(&[(1, "a")]), pages(&[(1, "b")]), &[]).unwrap();
    assert!(report.changed_cells.is_empty());
}

#[test]
fn keyed_compare_ignores_row_order_and_uses_only_named_keys() {
    let report = run(pages(&[(1, "a"), (2, "b")]), pages(&[(2, "b"), (1, "a")]), &["id"]).unwrap();
    assert_eq!((report.summary.equal, report.summary.changed), (2, 0));
}

#[test]
fn a_duplicate_key_on_either_side_is_refused() {
    let left = run(pages(&[(1, "a"), (1, "b")]), pages(&[(1, "a")]), &["id"]);
    assert_eq!(left.unwrap_err(), CompareError::DuplicateKey);
    let right = run(pages(&[(1, "a")]), pages(&[(1, "a"), (1, "b")]), &["id"]);
    assert_eq!(right.unwrap_err(), CompareError::DuplicateKey);
}

#[test]
fn a_duplicate_key_on_the_right_that_the_left_lacks_is_refused() {
    let right = run(
        pages(&[(1, "a"), (2, "b")]),
        pages(&[(1, "a"), (2, "b"), (3, "c"), (3, "d")]),
        &["id"],
    );
    assert_eq!(right.unwrap_err(), CompareError::DuplicateKey);
}

#[test]
fn a_null_or_undecodable_key_is_refused() {
    let with_null = CompareInput::Pages(vec![result(
        &["id", "label"],
        vec![vec![Value::Null, Value::Text("x".into())]],
    )]);
    let error = run(with_null, pages(&[(1, "a")]), &["id"]).unwrap_err();
    assert_eq!(error, CompareError::UnreadableKey);
    let bad = vec![vec![Value::Undecodable("numeric".into()), Value::Text("x".into())]];
    let right = CompareInput::Pages(vec![result(&["id", "label"], bad)]);
    assert_eq!(
        run(pages(&[(1, "a")]), right, &["id"]).unwrap_err(),
        CompareError::UnreadableKey
    );
}

#[test]
fn an_unknown_key_column_is_refused() {
    assert_eq!(
        run(pages(&[(1, "a")]), pages(&[(1, "a")]), &["missing"]).unwrap_err(),
        CompareError::UnknownKeyColumn
    );
}

#[test]
fn unreadable_cells_make_a_row_unknown_not_equal_or_changed() {
    let bad = |label: &str| vec![vec![Value::Int(1), Value::Undecodable(label.into())]];
    let left = CompareInput::Pages(vec![result(&["id", "label"], bad("numeric"))]);
    let right = CompareInput::Pages(vec![result(&["id", "label"], bad("numeric"))]);
    let keyed = run(left, right, &["id"]).unwrap();
    assert_eq!((keyed.summary.equal, keyed.summary.changed), (0, 0));
    assert_eq!((keyed.summary.unknown_left, keyed.summary.unknown_right), (1, 1));
    let left = CompareInput::Pages(vec![result(&["id", "label"], bad("numeric"))]);
    let right = CompareInput::Pages(vec![result(&["id", "label"], bad("numeric"))]);
    let whole = run(left, right, &[]).unwrap();
    assert_eq!(
        (whole.summary.equal, whole.summary.added, whole.summary.removed),
        (0, 0, 0)
    );
    assert_eq!((whole.summary.unknown_left, whole.summary.unknown_right), (1, 1));
}

#[test]
fn columns_are_matched_by_name_and_extras_are_reported() {
    let left = CompareInput::Pages(vec![result(
        &["label", "id", "only_left"],
        vec![vec![Value::Text("a".into()), Value::Int(1), Value::Int(9)]],
    )]);
    let right = CompareInput::Pages(vec![result(
        &["id", "label", "only_right"],
        vec![vec![Value::Int(1), Value::Text("a".into()), Value::Int(8)]],
    )]);
    let report = run(left, right, &[]).unwrap();
    assert_eq!(report.summary.equal, 1);
    assert_eq!(report.columns_only_left, vec!["only_left".to_string()]);
    assert_eq!(report.columns_only_right, vec!["only_right".to_string()]);
}

#[test]
fn results_without_shared_or_with_duplicate_column_names_are_refused() {
    let left = CompareInput::Pages(vec![result(&["a"], vec![vec![Value::Int(1)]])]);
    let right = CompareInput::Pages(vec![result(&["b"], vec![vec![Value::Int(1)]])]);
    assert_eq!(run(left, right, &[]).unwrap_err(), CompareError::NoCommonColumns);
    let twice = CompareInput::Pages(vec![result(&["a", "a"], vec![vec![Value::Int(1), Value::Int(1)]])]);
    let once = CompareInput::Pages(vec![result(&["a"], vec![vec![Value::Int(1)]])]);
    assert_eq!(run(twice, once, &[]).unwrap_err(), CompareError::AmbiguousColumns);
}

#[test]
fn a_truncated_input_is_refused() {
    let truncated = Arc::new(QueryResult {
        columns: vec![column("id")],
        rows: vec![vec![Value::Int(1)]].into(),
        truncated: true,
    });
    let right = CompareInput::Pages(vec![result(&["id"], vec![vec![Value::Int(1)]])]);
    assert_eq!(
        run(CompareInput::Pages(vec![truncated]), right, &[]).unwrap_err(),
        CompareError::IncompleteInput
    );
}

#[test]
fn a_cancelled_comparison_returns_cancelled() {
    let request = CompareRequest {
        left: pages(&[(1, "a")]),
        right: pages(&[(1, "a")]),
        key_columns: Vec::new(),
    };
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = compare_results(&request, &cancel, &ScanProgress::default()).unwrap_err();
    assert_eq!(error, CompareError::Cancelled);
}

#[test]
fn differences_are_capped_but_still_counted() {
    let many: Vec<Vec<Value>> = (0..MAX_COMPARE_DIFFERENCES as i64 + 10)
        .map(|id| vec![Value::Int(id)])
        .collect();
    let right = CompareInput::Pages(vec![result(&["id"], many)]);
    let left = CompareInput::Pages(vec![result(&["id"], Vec::new())]);
    let report = run(left, right, &[]).unwrap();
    assert_eq!(report.summary.added, MAX_COMPARE_DIFFERENCES as u64 + 10);
    assert_eq!(report.diff.rows.len(), MAX_COMPARE_DIFFERENCES);
    assert!(report.summary.differences_capped);
}

#[test]
fn keyed_changed_rows_beyond_the_cap_never_leave_a_dangling_before() {
    let changed = MAX_COMPARE_DIFFERENCES / 2 + 10;
    let left: Vec<Vec<Value>> = (0..changed as i64)
        .map(|id| vec![Value::Int(id), Value::Text("a".into())])
        .collect();
    let mut right: Vec<Vec<Value>> = (0..changed as i64)
        .map(|id| vec![Value::Int(id), Value::Text("b".into())])
        .collect();
    right.push(vec![Value::Int(-1), Value::Text("new".into())]);
    let input = |rows| CompareInput::Pages(vec![result(&["id", "label"], rows)]);
    let report = run(input(left), input(right), &["id"]).unwrap();
    assert_eq!(report.summary.changed, changed as u64);
    assert_eq!(report.summary.added, 1);
    assert!(report.summary.differences_capped);
    let rows = changes(&report);
    assert!(rows.len() <= MAX_COMPARE_DIFFERENCES);
    let mut index = 0;
    while index < rows.len() {
        match rows[index].0.as_str() {
            "Before" => {
                assert_eq!(rows.get(index + 1).map(|row| row.0.as_str()), Some("After"));
                index += 2;
            }
            _ => index += 1,
        }
    }
}

fn spooled(rows_per_page: &[&[(i64, &str)]]) -> CompareInput {
    let spool = StreamResultSpoolManager::default().create().unwrap();
    let mut total = 0_u64;
    for (index, rows) in rows_per_page.iter().enumerate() {
        total += rows.len() as u64;
        let page = result(&["id", "label"], pairs(rows));
        spool.append_page_sync(index as u64, &page).unwrap();
    }
    CompareInput::Spool(ScanSource::new(spool, vec![column("id"), column("label")], total))
}

#[test]
fn spooled_and_in_memory_results_compare_the_same() {
    let in_memory = pages(&[(1, "a"), (2, "b"), (3, "c")]);
    let from_spool = spooled(&[&[(1, "a"), (2, "x")], &[(4, "d")]]);
    let report = run(in_memory, from_spool, &["id"]).unwrap();
    let summary = &report.summary;
    assert_eq!(
        (summary.equal, summary.changed, summary.added, summary.removed),
        (1, 1, 1, 1)
    );
}

#[test]
fn two_spooled_results_compare_across_pages() {
    let left = spooled(&[&[(1, "a"), (2, "b")], &[(3, "c")]]);
    let right = spooled(&[&[(3, "c")], &[(1, "a"), (2, "b")]]);
    let report = run(left, right, &[]).unwrap();
    assert_eq!(
        (
            report.summary.equal,
            report.summary.rows_left,
            report.summary.rows_right
        ),
        (3, 3, 3)
    );
}

#[test]
fn a_spooled_input_with_a_wrong_row_total_is_incomplete() {
    let spool = StreamResultSpoolManager::default().create().unwrap();
    spool
        .append_page_sync(0, &result(&["id", "label"], pairs(&[(1, "a")])))
        .unwrap();
    let broken = CompareInput::Spool(ScanSource::new(spool, vec![column("id"), column("label")], 5));
    assert_eq!(
        run(broken, pages(&[(1, "a")]), &[]).unwrap_err(),
        CompareError::IncompleteInput
    );
}

#[test]
fn shared_column_names_follow_the_left_order_and_refuse_ambiguity() {
    let left = [column("b"), column("a"), column("only_left")];
    let right = [column("a"), column("b"), column("only_right")];
    assert_eq!(
        shared_column_names(&left, &right).unwrap(),
        vec!["b".to_string(), "a".to_string()]
    );
    assert_eq!(
        shared_column_names(&[column("a"), column("a")], &right).unwrap_err(),
        CompareError::AmbiguousColumns
    );
    assert_eq!(
        shared_column_names(&[column("z")], &right).unwrap_err(),
        CompareError::NoCommonColumns
    );
}
