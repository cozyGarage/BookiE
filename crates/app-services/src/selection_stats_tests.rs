use bookie_core::{ColumnInfo, Value};
use rust_decimal::Decimal;

use super::{ColumnStats, StatsAccumulator, column_stats, column_stats_in, tooltip};

fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: String::new(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

#[test]
fn integers_sum_exactly_and_nulls_are_skipped() {
    let rows = vec![vec![Value::Int(1)], vec![Value::Null], vec![Value::Int(4)]];
    assert_eq!(
        column_stats(&[column("n")], &rows),
        [ColumnStats {
            column: "n".into(),
            count: 2,
            minimum: "1".into(),
            maximum: "4".into(),
            sum: "5".into(),
            average: "2.5".into()
        }]
    );
}

#[test]
fn statistics_can_be_limited_to_chosen_columns() {
    let rows = [vec![Value::Int(1), Value::Int(10)], vec![Value::Int(2), Value::Int(20)]];
    let stats = column_stats_in(&[column("a"), column("b")], &rows, &[1]);
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].column, "b");
    assert_eq!(stats[0].sum, "30");
}

#[test]
fn decimals_keep_their_precision() {
    let d = |s: &str| Value::Decimal(s.parse().unwrap());
    let stats = column_stats(&[column("p")], &[vec![d("0.1")], vec![d("0.2")]]);
    assert_eq!(stats[0].sum, "0.3");
    assert_eq!(stats[0].minimum, "0.1");
    assert_eq!(stats[0].maximum, "0.2");
}

#[test]
fn non_finite_and_overflowing_float_summaries_are_omitted() {
    assert!(column_stats(&[column("n")], &[vec![Value::Float(f64::INFINITY)]]).is_empty());
    let overflow = [vec![Value::Float(f64::MAX)], vec![Value::Float(f64::MAX)]];
    assert!(column_stats(&[column("n")], &overflow).is_empty());
}

#[test]
fn mixed_or_text_columns_have_no_stats() {
    let rows = vec![
        vec![Value::Int(1), Value::Text("a".into())],
        vec![Value::Float(1.0), Value::Text("b".into())],
    ];
    assert!(column_stats(&[column("a"), column("b")], &rows).is_empty());
}

#[test]
fn an_all_null_column_has_no_stats() {
    assert!(column_stats(&[column("n")], &[vec![Value::Null]]).is_empty());
}

#[test]
fn wide_non_numeric_cells_do_not_change_exact_statistics() {
    let rows = vec![
        vec![Value::Int(9), Value::Text("x".repeat(1024 * 1024))],
        vec![Value::Null, Value::Bytes(vec![7; 1024 * 1024])],
        vec![Value::Int(3), Value::Text("y".repeat(1024 * 1024))],
    ];
    let stats = column_stats(&[column("number"), column("payload")], &rows);
    assert_eq!(stats.len(), 1);
    assert_eq!(
        (stats[0].count, stats[0].sum.as_str(), stats[0].average.as_str()),
        (2, "12", "6")
    );
    assert_eq!((stats[0].minimum.as_str(), stats[0].maximum.as_str()), ("3", "9"));
}

#[test]
fn overflowing_decimal_summaries_are_omitted() {
    let rows = [vec![Value::Decimal(Decimal::MAX)], vec![Value::Decimal(Decimal::ONE)]];
    assert!(column_stats(&[column("n")], &rows).is_empty());
}

#[test]
fn an_accumulator_fed_cell_by_cell_matches_the_row_based_result() {
    let mut accumulator = StatsAccumulator::default();
    for value in [Value::Int(2), Value::Null, Value::Int(6)] {
        accumulator.push(&value);
    }
    let stats = accumulator.finish("n").expect("numeric");
    assert_eq!((stats.count, stats.sum.as_str(), stats.average.as_str()), (2, "8", "4"));
}

#[test]
fn a_cell_that_could_not_be_read_leaves_the_column_without_stats() {
    let mut accumulator = StatsAccumulator::default();
    accumulator.push(&Value::Int(1));
    accumulator.unreadable();
    accumulator.push(&Value::Int(2));
    assert!(accumulator.finish("n").is_none());
}

#[test]
fn the_tooltip_lists_one_line_per_column() {
    let rows = [vec![Value::Int(1), Value::Int(5)], vec![Value::Int(3), Value::Int(7)]];
    let text = tooltip(&column_stats(&[column("a"), column("b")], &rows));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(
        lines[0].starts_with("a: count 2, min 1, max 3, sum 4, average 2"),
        "{}",
        lines[0]
    );
    assert!(!text.contains('{'));
}
