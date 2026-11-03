use std::str::FromStr;

use bookie_core::Value;
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use rust_decimal::Decimal;

use super::exact_equal;
use crate::change_tracker::KeyValue;

fn samples() -> Vec<Value> {
    vec![
        Value::Null,
        Value::Bool(true),
        Value::Bool(false),
        Value::Int(0),
        Value::Int(1),
        Value::Float(0.0),
        Value::Float(-0.0),
        Value::Float(f64::NAN),
        Value::Float(f64::from_bits(f64::NAN.to_bits() | 1)),
        Value::Float(1.0),
        Value::Text(String::new()),
        Value::Text("a".into()),
        Value::JsonText("a".into()),
        Value::Bytes(Vec::new()),
        Value::Bytes(vec![0]),
        Value::Date(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()),
        Value::Time(NaiveTime::from_hms_opt(1, 2, 3).unwrap()),
        Value::TimestampTz(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
        Value::Decimal(Decimal::from_str("1.0").unwrap()),
        Value::Decimal(Decimal::from_str("1.00").unwrap()),
        Value::Decimal(Decimal::from_str("1").unwrap()),
        Value::Decimal(Decimal::from_str("0.0").unwrap()),
        Value::Decimal(Decimal::from_str("-0.0").unwrap()),
        Value::Json(Box::new(serde_json::json!({"a": 1, "b": [1, 2]}))),
        Value::Json(Box::new(serde_json::json!({"b": [1, 2], "a": 1}))),
        Value::Undecodable("numeric".into()),
        Value::Undecodable("money".into()),
    ]
}

#[test]
fn exact_equality_agrees_with_the_key_value_identity_for_every_pair() {
    let values = samples();
    for left in &values {
        for right in &values {
            assert_eq!(
                exact_equal(left, right),
                KeyValue::from(left) == KeyValue::from(right),
                "{left:?} vs {right:?}"
            );
        }
    }
}
