use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::model::{ColumnMeta, Diff, FindResult, Predicate};

struct Rows {
    columns: Vec<ColumnMeta>,
    row: Vec<Cell>,
    count: u64,
    incomplete: bool,
    reads: AtomicUsize,
    borrowed: bool,
    cancel_after_read: Option<CancellationToken>,
}

impl Rows {
    fn new(row: Vec<Cell>, count: u64) -> Self {
        let columns = row
            .iter()
            .enumerate()
            .map(|(index, _)| ColumnMeta {
                name: format!("field_{index}"),
                width: 100.0,
                numeric: false,
                data_type: String::new(),
                primary_key: false,
            })
            .collect();
        Self {
            columns,
            row,
            count,
            incomplete: false,
            reads: AtomicUsize::new(0),
            borrowed: true,
            cancel_after_read: None,
        }
    }
}

impl ResultSource for Rows {
    fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }
    fn total(&self) -> u64 {
        self.count
    }
    fn is_partial(&self) -> bool {
        self.incomplete
    }
    fn row(&self, _: u64) -> Vec<Cell> {
        assert!(!self.borrowed, "borrowed rows must not use the copying accessor");
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.row.clone()
    }
    fn row_ref(&self, _: u64) -> Option<&[Cell]> {
        if !self.borrowed {
            return None;
        }
        self.reads.fetch_add(1, Ordering::Relaxed);
        if let Some(cancel) = &self.cancel_after_read {
            cancel.cancel();
        }
        Some(&self.row)
    }
    fn find(&self, _: &str, _: usize, _: &CancellationToken) -> Result<FindResult, &'static str> {
        Err("unused")
    }
    fn filter(&self, _: &Predicate, _: &CancellationToken) -> Result<Vec<u32>, &'static str> {
        Err("unused")
    }
    fn compare(&self, _: bool, _: &CancellationToken) -> Result<Diff, &'static str> {
        Err("unused")
    }
}

#[test]
fn a_chart_keeps_at_most_the_row_limit_and_reports_sampling() {
    let source = Rows::new(
        vec![Cell::Text("group".into()), Cell::Int(2)],
        result_chart::MAX_ROWS as u64 + 1,
    );
    let chart = ChartState::load(&source, &CancellationToken::new()).unwrap();
    assert_eq!(chart.rows.len(), result_chart::MAX_ROWS);
    assert!(chart.data.rows_cut);
    assert_eq!(chart.total, result_chart::MAX_ROWS as u64 + 1);
    assert_eq!(source.reads.load(Ordering::Relaxed), result_chart::MAX_ROWS);
    assert_eq!(chart.data.series[0].values, [Some(2.0 * result_chart::MAX_ROWS as f64)]);
}

#[test]
fn wide_rows_stop_at_the_estimated_byte_budget() {
    let source = Rows::new(vec![Cell::Text("x".repeat(1024 * 1024)), Cell::Int(1)], 100);
    let expected = MAX_CHART_BYTES / bookie_core::estimated_row_size(&source.row);
    let chart = ChartState::load(&source, &CancellationToken::new()).unwrap();
    assert_eq!(chart.rows.len(), expected);
    assert!(chart.data.rows_cut);
    assert_eq!(source.reads.load(Ordering::Relaxed), expected + 1);
    let bytes: usize = chart.rows.iter().map(|row| bookie_core::estimated_row_size(row)).sum();
    assert!(bytes <= MAX_CHART_BYTES);
}

#[test]
fn an_oversized_borrowed_first_row_is_not_copied_or_claimed_complete() {
    let source = Rows::new(vec![Cell::Text("x".repeat(MAX_CHART_BYTES + 1))], 10_000);
    let chart = ChartState::load(&source, &CancellationToken::new()).unwrap();
    assert!(chart.rows.is_empty());
    assert!(chart.data.rows_cut);
    assert_eq!(chart.data.rows, 0);
    assert_eq!(source.reads.load(Ordering::Relaxed), 1);
}

#[test]
fn cancellation_stops_before_reading_or_copying_rows() {
    let source = Rows::new(vec![Cell::Int(1)], 100);
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(ChartState::load(&source, &cancel), Err(ChartError::Cancelled)));
    assert_eq!(source.reads.load(Ordering::Relaxed), 0);
}

#[test]
fn a_missing_or_mismatched_row_is_an_error_instead_of_a_null_group() {
    let mut source = Rows::new(vec![Cell::Int(1)], 1);
    source.row.clear();
    assert!(matches!(
        ChartState::load(&source, &CancellationToken::new()),
        Err(ChartError::UnavailableRow)
    ));
}

#[test]
fn incomplete_sources_stay_incomplete_even_when_all_retained_rows_fit() {
    let mut source = Rows::new(vec![Cell::Text("group".into()), Cell::Int(1)], 2);
    source.incomplete = true;
    let chart = ChartState::load(&source, &CancellationToken::new()).unwrap();
    assert!(chart.incomplete);
    assert!(!chart.data.rows_cut);
    assert_eq!(chart.data.rows, 2);
}

#[test]
fn full_values_are_kept_for_grouping_and_released_with_the_chart() {
    let source = Rows::new(vec![Cell::Text("💾".repeat(1_000)), Cell::Int(7)], 2);
    let chart = ChartState::load(&source, &CancellationToken::new()).unwrap();
    assert!(
        chart.rows[0] == source.row,
        "chart sampling preserves the full typed row"
    );
    assert_eq!(chart.data.labels, [Some("💾".repeat(1_000))]);
    let rows = Arc::downgrade(&chart.rows);
    let data = Arc::downgrade(&chart.data);
    drop(chart);
    assert!(rows.upgrade().is_none());
    assert!(data.upgrade().is_none());
}

#[test]
fn cancellation_during_sampling_drops_the_partial_snapshot_and_stops_reading() {
    let cancel = CancellationToken::new();
    let mut source = Rows::new(vec![Cell::Text("group".into()), Cell::Int(1)], 100);
    source.cancel_after_read = Some(cancel.clone());
    assert!(matches!(ChartState::load(&source, &cancel), Err(ChartError::Cancelled)));
    assert_eq!(source.reads.load(Ordering::Relaxed), 1);
}

#[test]
fn owned_row_sources_keep_exact_values_and_the_same_bounds() {
    let mut source = Rows::new(vec![Cell::Text("x".repeat(1024 * 1024)), Cell::Int(7)], 100);
    source.borrowed = false;
    let chart = ChartState::load(&source, &CancellationToken::new()).unwrap();
    assert!(
        chart.rows[0] == source.row,
        "chart sampling preserves the full typed row"
    );
    assert_eq!(
        chart.rows.len(),
        MAX_CHART_BYTES / bookie_core::estimated_row_size(&source.row)
    );
    assert!(chart.data.rows_cut);
    assert_eq!(source.reads.load(Ordering::Relaxed), chart.rows.len() + 1);
}

#[test]
fn an_empty_result_needs_no_row_read_and_is_not_reported_as_sampled() {
    let source = Rows::new(vec![Cell::Int(1)], 0);
    let chart = ChartState::load(&source, &CancellationToken::new()).unwrap();
    assert_eq!(chart.data.rows, 0);
    assert!(chart.rows.is_empty());
    assert!(!chart.data.rows_cut);
    assert_eq!(source.reads.load(Ordering::Relaxed), 0);
}
