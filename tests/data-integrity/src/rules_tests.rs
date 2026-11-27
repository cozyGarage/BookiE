use std::str::FromStr;

use bookie_core::Value;
use chrono::{NaiveDate, TimeZone, Utc};
use rust_decimal::Decimal;

use super::*;

#[test]
fn a_decimal_must_keep_the_servers_scale() {
    let value = Value::Decimal(Decimal::from_str("1.2300").unwrap());
    assert!(check(Rule::Exact, Carrier::Decimal, &value, Some("1.2300")).is_ok());
    assert!(check(Rule::Exact, Carrier::Decimal, &value, Some("1.23")).is_err());
}

#[test]
fn a_wrong_carrier_fails_even_when_the_text_matches() {
    let value = Value::Text("1.23".into());
    assert!(check(Rule::Exact, Carrier::Decimal, &value, Some("1.23")).is_err());
}

#[test]
fn null_empty_and_text_null_stay_distinct() {
    assert!(check(Rule::Exact, Carrier::Text, &Value::Null, None).is_ok());
    assert!(check(Rule::Exact, Carrier::Text, &Value::Null, Some("")).is_err());
    assert!(check(Rule::Exact, Carrier::Text, &Value::Text(String::new()), None).is_err());
    assert!(check(Rule::Exact, Carrier::Text, &Value::Text("NULL".into()), Some("NULL")).is_ok());
    assert!(check(Rule::Exact, Carrier::Text, &Value::Null, Some("NULL")).is_err());
}

#[test]
fn floats_compare_by_bits_so_negative_zero_and_nan_are_exact() {
    assert!(check(Rule::Float, Carrier::Float, &Value::Float(-0.0), Some("-0")).is_ok());
    assert!(check(Rule::Float, Carrier::Float, &Value::Float(0.0), Some("-0")).is_err());
    assert!(check(Rule::Float, Carrier::Float, &Value::Float(f64::NAN), Some("NaN")).is_ok());
    assert!(
        check(
            Rule::Float,
            Carrier::Float,
            &Value::Float(f64::INFINITY),
            Some("Infinity")
        )
        .is_ok()
    );
    assert!(check(Rule::Float, Carrier::Float, &Value::Float(0.1), Some("0.1")).is_ok());
    assert!(
        check(
            Rule::Float,
            Carrier::Float,
            &Value::Float(0.1),
            Some("0.10000000000000002")
        )
        .is_err()
    );
}

#[test]
fn temporal_text_ignores_only_trailing_fraction_zeros() {
    let stamp = NaiveDate::from_ymd_opt(2024, 2, 29)
        .unwrap()
        .and_hms_micro_opt(13, 45, 30, 100_000)
        .unwrap();
    let value = Value::DateTime(stamp);
    assert!(check(Rule::Temporal, Carrier::DateTime, &value, Some("2024-02-29 13:45:30.1")).is_ok());
    assert!(
        check(
            Rule::Temporal,
            Carrier::DateTime,
            &value,
            Some("2024-02-29 13:45:30.100000")
        )
        .is_ok()
    );
    assert!(
        check(
            Rule::Temporal,
            Carrier::DateTime,
            &value,
            Some("2024-02-29 13:45:30.01")
        )
        .is_err()
    );
    assert_eq!(trim_fraction("12:00:00.000 BC"), "12:00:00 BC");
}

#[test]
fn instants_compare_as_exact_epoch_decimals() {
    let stamp = Utc.timestamp_opt(-2, 500_000_000).unwrap();
    assert_eq!(epoch_text(&stamp), "-1.5");
    let value = Value::TimestampTz(stamp);
    assert!(check(Rule::Instant, Carrier::TimestampTz, &value, Some("-1.500000")).is_ok());
    assert!(check(Rule::Instant, Carrier::TimestampTz, &value, Some("-1.499999")).is_err());
    assert_eq!(canonical_decimal("-0.000"), "0");
    assert_eq!(canonical_decimal("001.10"), "1.1");
}

#[test]
fn bytes_compare_as_hex() {
    let value = Value::Bytes(vec![0, 0xde, 0xad]);
    assert!(check(Rule::Bytes, Carrier::Bytes, &value, Some("00DEAD")).is_ok());
    assert!(check(Rule::Bytes, Carrier::Bytes, &value, Some("DEAD")).is_err());
}

#[test]
fn a_refusal_accepts_only_an_undecodable_cell() {
    assert!(
        check(
            Rule::Refusal,
            Carrier::Undecodable,
            &Value::Undecodable("x".into()),
            Some("1")
        )
        .is_ok()
    );
    assert!(check(Rule::Refusal, Carrier::Undecodable, &Value::Text("1".into()), Some("1")).is_err());
}

#[test]
fn json_compares_normalized_documents_without_reordering_keys() {
    let value = Value::Json(Box::new(serde_json::json!({"b": 1, "a": 2})));
    assert!(check(Rule::Json, Carrier::Json, &value, Some(r#"{"b": 1, "a": 2}"#)).is_ok());
    assert!(check(Rule::Json, Carrier::Json, &value, Some(r#"{"a": 2, "b": 1}"#)).is_err());
}

#[test]
fn number_ignores_only_leading_and_trailing_zeros() {
    assert!(check(Rule::Number, Carrier::Int, &Value::Int(0), Some("0000")).is_ok());
    assert!(check(Rule::Number, Carrier::Int, &Value::Int(1901), Some("1901")).is_ok());
    assert!(check(Rule::Number, Carrier::Int, &Value::Int(1901), Some("1902")).is_err());
}

#[test]
fn a_date_or_wall_clock_compares_as_its_utc_epoch() {
    let day = NaiveDate::from_ymd_opt(-4712, 1, 1).unwrap();
    assert!(check(Rule::Instant, Carrier::Date, &Value::Date(day), Some("-210863520000")).is_ok());
}

#[test]
fn float32_compares_at_single_precision() {
    let widened_shortest = Value::Float(1.1754944e-38);
    assert!(
        check(
            Rule::Float32,
            Carrier::Float,
            &widened_shortest,
            Some("1.1754943508222875e-38")
        )
        .is_ok()
    );
    assert!(
        check(
            Rule::Float,
            Carrier::Float,
            &widened_shortest,
            Some("1.1754943508222875e-38")
        )
        .is_err()
    );
    assert!(
        check(
            Rule::Float32,
            Carrier::Float,
            &Value::Float(16777216.0),
            Some("16777218")
        )
        .is_err()
    );
}

#[test]
fn nanosecond_epochs_become_exact_seconds() {
    assert_eq!(nanos_to_seconds("1500000000"), "1.5");
    assert_eq!(nanos_to_seconds("-1"), "-0.000000001");
    assert_eq!(nanos_to_seconds("0"), "0");
}
