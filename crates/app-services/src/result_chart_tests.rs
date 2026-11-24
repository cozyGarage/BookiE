use std::str::FromStr;

use bookie_core::{ColumnInfo, Value};
use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::{
    Aggregate, Cut, Kind, MAX_BARS, MAX_POINTS, MAX_ROWS, Settings, compute, defaults, nice_ticks, profile, tick_format,
};

fn column(name: &str, data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
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

fn columns(specs: &[(&str, &str)]) -> Vec<ColumnInfo> {
    specs.iter().map(|(name, data_type)| column(name, data_type)).collect()
}

fn t(text: &str) -> Value {
    Value::Text(text.into())
}

fn n(value: i64) -> Value {
    Value::Int(value)
}

fn d(text: &str) -> Value {
    Value::Decimal(Decimal::from_str(text).unwrap())
}

fn values(settings: &Settings) -> Vec<usize> {
    settings.picked().map(|(_, column)| column).collect()
}

fn chart(specs: &[(&str, &str)], rows: &[Vec<Value>]) -> super::ChartData {
    let profiles = profile(&columns(specs), rows);
    compute(rows, &profiles, &defaults(&profiles))
}

#[test]
fn only_labels_that_repeat_have_rows_to_combine() {
    let specs = [("region", "TEXT"), ("units", "INTEGER")];
    assert!(!chart(&specs, &[vec![t("North"), n(3)], vec![t("South"), n(4)]]).combines());
    let rows = [vec![t("North"), n(3)], vec![t("North"), n(4)], vec![t("South"), n(1)]];
    assert!(chart(&specs, &rows).combines());
}

#[test]
fn labels_that_dont_repeat_plot_their_own_values() {
    let rows = [vec![t("North"), n(3)], vec![t("South"), n(4)]];
    let profiles = profile(&columns(&[("region", "TEXT"), ("units", "INTEGER")]), &rows);
    let settings = Settings {
        aggregate: Aggregate::Count,
        ..defaults(&profiles)
    };
    assert_eq!(compute(&rows, &profiles, &settings).series[0].values, [Some(3.), Some(4.)]);
}

#[test]
fn defaults_label_by_repeating_text_and_measure_the_last_number_that_isnt_a_key() {
    let specs = [
        ("id", "INTEGER"),
        ("region", "TEXT"),
        ("units", "INTEGER"),
        ("revenue", "REAL"),
        ("customer_id", "INTEGER"),
    ];
    let rows = [
        vec![n(1), t("North"), n(3), Value::Float(10.5), n(7)],
        vec![n(2), t("South"), n(4), Value::Float(12.), n(8)],
        vec![n(3), t("North"), n(1), Value::Float(3.), n(9)],
    ];
    let settings = defaults(&profile(&columns(&specs), &rows));
    assert_eq!((settings.label, values(&settings)), (1, vec![3]));
    assert_eq!((settings.kind, settings.aggregate), (Kind::Bars, Aggregate::Sum));
}

#[test]
fn a_primary_key_is_not_a_measure() {
    let mut specs = columns(&[("code", "INTEGER"), ("name", "TEXT")]);
    specs[0].primary_key = true;
    let rows = [vec![n(1), t("a")], vec![n(2), t("b")]];
    let settings = defaults(&profile(&specs, &rows));
    assert_eq!((values(&settings), settings.aggregate), (vec![], Aggregate::Count));
}

#[test]
fn a_key_labels_its_measure_when_nothing_else_can() {
    let rows = [vec![n(1), n(4)], vec![n(2), n(9)]];
    let settings = defaults(&profile(&columns(&[("customer_id", "INTEGER"), ("orders", "BIGINT")]), &rows));
    assert_eq!((settings.label, values(&settings)), (0, vec![1]));
}

#[test]
fn dates_draw_a_line_and_sort_in_time() {
    let day = |m, d| Value::Date(NaiveDate::from_ymd_opt(2026, m, d).unwrap());
    let rows = [vec![day(3, 2), n(5)], vec![day(1, 15), n(2)], vec![t("2026-02-01"), n(4)]];
    let profiles = profile(&columns(&[("day", ""), ("total", "")]), &rows);
    assert!(profiles[0].temporal);
    let settings = defaults(&profiles);
    assert_eq!((settings.kind, settings.label), (Kind::Line, 0));
    let data = compute(&rows, &profiles, &settings);
    let labels: Vec<_> = data.labels.iter().map(|l| l.as_deref().unwrap()).collect();
    assert_eq!(labels, ["2026-01-15", "2026-02-01", "2026-03-02"]);
    assert_eq!(data.series[0].values, [Some(2.), Some(4.), Some(5.)]);
}

#[test]
fn numbers_come_from_typed_values_never_from_text() {
    let rows = [vec![t("a"), d("1.50"), t("1000")], vec![t("b"), d("2.25"), t("2000")]];
    let profiles = profile(&columns(&[("name", "text"), ("price", "numeric"), ("zip", "numeric")]), &rows);
    assert!(profiles[1].numeric && !profiles[2].numeric);
    assert_eq!(values(&defaults(&profiles)), [1]);
}

#[test]
fn decimals_sum_and_average_exactly() {
    let rows = [vec![t("a"), d("0.1")], vec![t("a"), d("0.2")], vec![t("b"), d("1")]];
    let profiles = profile(&columns(&[("name", "text"), ("price", "numeric")]), &rows);
    let mut settings = defaults(&profiles);
    assert_eq!(compute(&rows, &profiles, &settings).series[0].values, [Some(0.3), Some(1.)]);
    settings.aggregate = Aggregate::Average;
    assert_eq!(compute(&rows, &profiles, &settings).series[0].values, [Some(0.15), Some(1.)]);
}

#[test]
fn without_numbers_the_chart_counts_rows() {
    let rows = [vec![t("Sofia")], vec![t("Berlin")], vec![t("Sofia")]];
    let profiles = profile(&columns(&[("city", "TEXT")]), &rows);
    let settings = defaults(&profiles);
    assert_eq!((settings.aggregate, values(&settings)), (Aggregate::Count, vec![]));
    let data = compute(&rows, &profiles, &settings);
    assert_eq!(data.series.len(), 1);
    assert_eq!(
        (data.series[0].column, &data.series[0].values),
        (None, &vec![Some(2.), Some(1.)])
    );
}

#[test]
fn keys_look_like_keys() {
    let specs = [
        ("id", "int"),
        ("customerId", "int"),
        ("order_uuid", "text"),
        ("ref", "uuid"),
        ("paid", "int"),
        ("grid", "int"),
    ];
    let rows = [vec![n(1), n(1), t("x"), t("y"), n(1), n(1)]];
    let keys: Vec<bool> = profile(&columns(&specs), &rows).iter().map(|p| p.id_like).collect();
    assert_eq!(keys, [true, true, true, true, false, false]);
}

#[test]
fn sum_count_and_average_skip_nulls() {
    let rows = [
        vec![t("Sofia"), n(3)],
        vec![t("Berlin"), n(2)],
        vec![t("Sofia"), n(5)],
        vec![t("Sofia"), Value::Null],
        vec![Value::Null, n(1)],
    ];
    let profiles = profile(&columns(&[("city", "TEXT"), ("sales", "INTEGER")]), &rows);
    let mut settings = defaults(&profiles);
    let mut run = |aggregate| {
        settings.aggregate = aggregate;
        compute(&rows, &profiles, &settings)
    };
    assert_eq!(run(Aggregate::Sum).series[0].values, [Some(8.), Some(2.), Some(1.)]);
    assert_eq!(run(Aggregate::Count).series[0].values, [Some(2.), Some(1.), Some(1.)]);
    let data = run(Aggregate::Average);
    assert_eq!(data.series[0].values, [Some(4.), Some(2.), Some(1.)]);
    assert_eq!(data.labels, [Some("Sofia".into()), Some("Berlin".into()), None]);
}

#[test]
fn a_label_with_only_null_values_plots_a_gap_and_nulls_do_not_make_a_column_numeric() {
    let specs = [("city", "TEXT"), ("sales", "INTEGER"), ("empty", "INTEGER")];
    let rows = [
        vec![t("Sofia"), Value::Null, Value::Null],
        vec![t("Berlin"), n(2), Value::Null],
        vec![t("Sofia"), Value::Null, Value::Null],
    ];
    let profiles = profile(&columns(&specs), &rows);
    assert!(profiles[1].numeric && !profiles[2].numeric);
    let mut settings = defaults(&profiles);
    assert_eq!(values(&settings), [1]);
    assert_eq!(compute(&rows, &profiles, &settings).series[0].values, [None, Some(2.)]);
    settings.aggregate = Aggregate::Count;
    assert_eq!(compute(&rows, &profiles, &settings).series[0].values, [Some(0.), Some(1.)]);
}

#[test]
fn many_bars_keep_the_biggest_in_the_results_order() {
    let rows: Vec<_> = (0..250).map(|i| vec![t(&format!("l{i}")), n(i)]).collect();
    let data = chart(&[("label", "TEXT"), ("n", "INTEGER")], &rows);
    assert_eq!((data.labels.len(), data.groups, data.cut), (MAX_BARS, 250, Some(Cut::Top)));
    assert_eq!(data.labels.first().unwrap().as_deref(), Some("l50"));
    assert_eq!(data.labels.last().unwrap().as_deref(), Some("l249"));
}

#[test]
fn many_points_keep_the_first_and_many_rows_are_capped() {
    let rows: Vec<_> = (0..MAX_ROWS as i64 + 5).map(|i| vec![t(&format!("l{i}")), n(i)]).collect();
    let profiles = profile(&columns(&[("label", "TEXT"), ("n", "INTEGER")]), &rows);
    let settings = Settings {
        kind: Kind::Line,
        ..defaults(&profiles)
    };
    let data = compute(&rows, &profiles, &settings);
    assert_eq!((data.rows, data.rows_cut, data.groups), (MAX_ROWS, true, MAX_ROWS));
    assert_eq!((data.labels.len(), data.cut), (MAX_POINTS, Some(Cut::First)));
    assert_eq!(data.labels.first().unwrap().as_deref(), Some("l0"));
}

#[test]
fn a_value_keeps_its_slot_while_others_come_and_go() {
    let mut settings = Settings {
        kind: Kind::Bars,
        label: 0,
        values: [Some(1), Some(2), None, None, None],
        aggregate: Aggregate::Sum,
    };
    settings.toggle_value(1);
    settings.toggle_value(3);
    assert_eq!(settings.values, [Some(3), Some(2), None, None, None]);
    for column in 4..7 {
        assert!(settings.toggle_value(column));
    }
    assert!(!settings.toggle_value(9));
}

#[test]
fn ticks_are_round_and_cover_the_range() {
    assert_eq!(nice_ticks(0., 97., 5), [0., 25., 50., 75., 100.]);
    assert_eq!(nice_ticks(-12., 40., 5), [-20., 0., 20., 40.]);
    assert_eq!(nice_ticks(0., 0., 5), [0., 0.25, 0.5, 0.75, 1.]);
    let ticks = nice_ticks(0.1, 0.7, 4);
    let expected = [0., 0.2, 0.4, 0.6, 0.8];
    assert!(ticks.len() == expected.len() && ticks.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-9));
    assert_eq!(tick_format(250_000., 50_000.), (1e3, "k", 0));
    assert_eq!(tick_format(3.5, 0.25), (1., "", 2));
    assert_eq!(tick_format(2_500_000., 250_000.), (1e6, "M", 2));
}
