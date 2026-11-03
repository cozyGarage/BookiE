use std::str::FromStr;

use bookie_core::Value;
use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::*;

fn matches(predicate: ResultPredicate, row: &[Value]) -> bool {
    predicate
        .prepare(row.len())
        .unwrap()
        .matches(row, &mut CellTally::default())
}

fn contains(needle: &str, scope: ColumnScope) -> ResultPredicate {
    ResultPredicate::Contains {
        scope,
        needle: needle.into(),
    }
}

#[test]
fn text_search_ignores_case_and_matches_any_column() {
    let row = [Value::Int(1), Value::Text("Hello World".into())];
    assert!(matches(contains("WORLD", ColumnScope::Any), &row));
    assert!(!matches(contains("WORLD", ColumnScope::Column(0)), &row));
    assert!(matches(contains("hello", ColumnScope::Column(1)), &row));
}

#[test]
fn text_search_handles_non_ascii_case_folding() {
    let row = [Value::Text("ÉCOLE de Zürich".into())];
    assert!(matches(contains("école", ColumnScope::Any), &row));
    assert!(matches(contains("ZÜRICH", ColumnScope::Any), &row));
}

#[test]
fn typed_values_search_by_their_canonical_text() {
    let row = [
        Value::Int(-42),
        Value::Decimal(Decimal::from_str("12.3400").unwrap()),
        Value::Date(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()),
        Value::Bool(true),
        Value::Bytes(vec![0xde, 0xad]),
        Value::Uuid(uuid::Uuid::from_u128(1)),
    ];
    for needle in ["-42", "12.3400", "2024-02-29", "true", "dead", "0000-000000000001"] {
        assert!(matches(contains(needle, ColumnScope::Any), &row), "{needle}");
    }
    assert!(!matches(contains("12.34000", ColumnScope::Any), &row));
}

#[test]
fn null_never_matches_text_and_undecodable_cells_are_counted() {
    let row = [
        Value::Null,
        Value::Undecodable("numeric".into()),
        Value::Text("x".into()),
    ];
    let prepared = contains("null", ColumnScope::Any).prepare(3).unwrap();
    let mut tally = CellTally::default();
    assert!(!prepared.matches(&row, &mut tally));
    assert_eq!(tally.unsearchable_cells, 1);
}

#[test]
fn empty_search_text_and_unknown_columns_are_refused() {
    assert_eq!(
        contains("", ColumnScope::Any).prepare(2).unwrap_err(),
        PredicateError::EmptyNeedle
    );
    assert_eq!(
        contains("a", ColumnScope::Column(2)).prepare(2).unwrap_err(),
        PredicateError::UnknownColumn
    );
    let equals = ResultPredicate::Equals {
        column: 5,
        value: Value::Int(1),
    };
    assert_eq!(equals.prepare(2).unwrap_err(), PredicateError::UnknownColumn);
}

#[test]
fn equality_is_exact_and_typed() {
    let row = [Value::Int(1), Value::Text("1".into()), Value::Float(0.0)];
    let equals = |column, value| ResultPredicate::Equals { column, value };
    assert!(matches(equals(0, Value::Int(1)), &row));
    assert!(!matches(equals(0, Value::Text("1".into())), &row));
    assert!(!matches(equals(1, Value::Int(1)), &row));
    assert!(!matches(equals(2, Value::Float(-0.0)), &row));
    assert!(matches(equals(2, Value::Float(0.0)), &row));
}

#[test]
fn equality_keeps_null_empty_text_and_empty_bytes_distinct() {
    let rows = [[Value::Null], [Value::Text(String::new())], [Value::Bytes(Vec::new())]];
    let equals = |value: Value| ResultPredicate::Equals { column: 0, value };
    let hits: Vec<Vec<bool>> = [Value::Null, Value::Text(String::new()), Value::Bytes(Vec::new())]
        .into_iter()
        .map(|value| rows.iter().map(|row| matches(equals(value.clone()), row)).collect())
        .collect();
    assert_eq!(hits[0], vec![true, false, false]);
    assert_eq!(hits[1], vec![false, true, false]);
    assert_eq!(hits[2], vec![false, false, true]);
}

#[test]
fn equality_preserves_nan_payloads_and_decimal_scale() {
    let quiet = f64::NAN;
    let other = f64::from_bits(quiet.to_bits() ^ 1);
    let row = [Value::Float(quiet), Value::Decimal(Decimal::from_str("1.0").unwrap())];
    let equals = |column, value| ResultPredicate::Equals { column, value };
    assert!(matches(equals(0, Value::Float(quiet)), &row));
    assert!(!matches(equals(0, Value::Float(other)), &row));
    assert!(matches(
        equals(1, Value::Decimal(Decimal::from_str("1.0").unwrap())),
        &row
    ));
    assert!(!matches(
        equals(1, Value::Decimal(Decimal::from_str("1.00").unwrap())),
        &row
    ));
}

#[test]
fn not_equals_keeps_null_cells_and_other_types() {
    let not = |value| ResultPredicate::NotEquals { column: 0, value };
    assert!(matches(not(Value::Int(1)), &[Value::Null]));
    assert!(matches(not(Value::Int(1)), &[Value::Int(2)]));
    assert!(!matches(not(Value::Int(1)), &[Value::Int(1)]));
    assert!(matches(not(Value::Null), &[Value::Int(1)]));
    assert!(!matches(not(Value::Null), &[Value::Null]));
}

#[test]
fn an_undecodable_comparison_value_is_refused() {
    let predicate = ResultPredicate::Equals {
        column: 0,
        value: Value::Undecodable("numeric".into()),
    };
    assert_eq!(predicate.prepare(1).unwrap_err(), PredicateError::UncomparableValue);
}
