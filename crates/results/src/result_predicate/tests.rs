use std::str::FromStr;

use bookie_core::Value;
use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::text_match::TextMatchOptions;

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
        options: TextMatchOptions::default(),
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
fn text_search_honours_match_case_whole_words_and_regex() {
    let row = [Value::Text("Hello World".into()), Value::Text("concatenate".into())];
    let cased = ResultPredicate::Contains {
        scope: ColumnScope::Any,
        needle: "hello".into(),
        options: TextMatchOptions {
            match_case: true,
            ..TextMatchOptions::default()
        },
    };
    assert!(!matches(cased, &row));
    let whole = ResultPredicate::Contains {
        scope: ColumnScope::Column(1),
        needle: "cat".into(),
        options: TextMatchOptions {
            whole_words: true,
            ..TextMatchOptions::default()
        },
    };
    assert!(!matches(whole, &row));
    let regex = ResultPredicate::Contains {
        scope: ColumnScope::Any,
        needle: r"W.rld".into(),
        options: TextMatchOptions {
            regex: true,
            ..TextMatchOptions::default()
        },
    };
    assert!(matches(regex, &row));
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

#[test]
fn temporal_values_search_as_the_grid_displays_them() {
    use chrono::{NaiveTime, TimeZone, Utc};
    let stamp = Utc.with_ymd_and_hms(2024, 5, 1, 10, 0, 0).unwrap();
    let row = [
        Value::TimestampTz(stamp),
        Value::Time(NaiveTime::from_hms_milli_opt(8, 30, 15, 123).unwrap()),
    ];
    assert!(matches(
        contains("2024-05-01 10:00:00+00:00", ColumnScope::Column(0)),
        &row
    ));
    assert!(matches(contains("2024-05-01 10:00", ColumnScope::Any), &row));
    assert!(matches(contains("08:30:15", ColumnScope::Column(1)), &row));
    assert!(!matches(contains("08:30:15.123", ColumnScope::Column(1)), &row));
}

#[test]
fn readable_bytes_search_as_text_and_as_hex() {
    let row = [Value::Bytes(b"hello".to_vec()), Value::Bytes(vec![0xde, 0xad])];
    assert!(matches(contains("HELLO", ColumnScope::Column(0)), &row));
    assert!(matches(contains("68656c6c6f", ColumnScope::Column(0)), &row));
    assert!(matches(contains("dead", ColumnScope::Column(1)), &row));
    assert!(!matches(contains("hello", ColumnScope::Column(1)), &row));
}

#[test]
fn an_ascii_needle_still_matches_characters_that_lowercase_to_ascii() {
    let row = [Value::Text("Temp \u{212A}elvin \u{0130}stanbul".into())];
    assert!(matches(contains("kelvin", ColumnScope::Any), &row));
    assert!(matches(contains("i\u{307}stanbul", ColumnScope::Any), &row));
    let plain = [Value::Text("Zürich Café".into())];
    assert!(matches(contains("zür", ColumnScope::Any), &plain));
    assert!(!matches(contains("kelvin", ColumnScope::Any), &plain));
}

#[test]
fn not_equals_drops_and_counts_unreadable_cells() {
    let predicate = ResultPredicate::NotEquals {
        column: 0,
        value: Value::Int(1),
    }
    .prepare(1)
    .unwrap();
    let mut tally = CellTally::default();
    assert!(!predicate.matches(&[Value::Undecodable("numeric".into())], &mut tally));
    assert_eq!(tally.unsearchable_cells, 1);
    assert!(predicate.matches(&[Value::Int(2)], &mut tally));
    assert_eq!(tally.unsearchable_cells, 1);
}

#[test]
fn equals_counts_unreadable_cells_and_treats_a_missing_field_as_absent() {
    let equals = ResultPredicate::Equals {
        column: 0,
        value: Value::Int(1),
    };
    let prepared = equals.prepare(1).unwrap();
    let mut tally = CellTally::default();
    assert!(!prepared.matches(&[Value::Undecodable("numeric".into())], &mut tally));
    assert_eq!(tally.unsearchable_cells, 1);
    let missing = Value::Undecodable(Value::MISSING_FIELD.into());
    assert!(!prepared.matches(std::slice::from_ref(&missing), &mut tally));
    assert_eq!(tally.unsearchable_cells, 1);
    let not = ResultPredicate::NotEquals {
        column: 0,
        value: Value::Int(1),
    }
    .prepare(1)
    .unwrap();
    assert!(not.matches(std::slice::from_ref(&missing), &mut tally));
    assert_eq!(tally.unsearchable_cells, 1);
    let search = contains("missing", ColumnScope::Any).prepare(1).unwrap();
    assert!(!search.matches(std::slice::from_ref(&missing), &mut tally));
    assert_eq!(tally.unsearchable_cells, 1);
}
