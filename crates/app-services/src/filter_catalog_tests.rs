use bookie_core::{FilterOp, FilterValue};

use super::{OPERATORS, ValueShape, operator, rule_from_text};

#[test]
fn every_operator_is_listed_once_with_its_own_label() {
    let mut labels: Vec<&str> = OPERATORS.iter().map(|entry| entry.label).collect();
    labels.sort();
    labels.dedup();
    assert_eq!(labels.len(), OPERATORS.len());
    let mut ops: Vec<String> = OPERATORS.iter().map(|entry| format!("{:?}", entry.op)).collect();
    ops.sort();
    ops.dedup();
    assert_eq!(ops.len(), 18, "all eighteen FilterOp variants are offered");
}

#[test]
fn the_shape_follows_what_the_sql_builder_expects() {
    assert_eq!(operator(FilterOp::IsNull).shape, ValueShape::None);
    assert_eq!(operator(FilterOp::Between).shape, ValueShape::Pair);
    assert_eq!(operator(FilterOp::In).shape, ValueShape::List);
    assert_eq!(operator(FilterOp::Contains).shape, ValueShape::Single);
}

#[test]
fn typed_text_becomes_the_value_the_operator_needs() {
    let single = rule_from_text("name", FilterOp::Eq, " Ada ").unwrap();
    assert_eq!(single.value, Some(FilterValue::Single("Ada".into())));
    let pair = rule_from_text("n", FilterOp::Between, "3, 9").unwrap();
    assert_eq!(pair.value, Some(FilterValue::Pair("3".into(), "9".into())));
    let list = rule_from_text("n", FilterOp::In, "1, 2 ,3,").unwrap();
    assert_eq!(
        list.value,
        Some(FilterValue::List(vec!["1".into(), "2".into(), "3".into()]))
    );
    let null = rule_from_text("n", FilterOp::IsNotNull, "ignored").unwrap();
    assert_eq!(null.value, None);
}

#[test]
fn missing_or_malformed_values_are_refused_with_a_reason() {
    assert!(rule_from_text("n", FilterOp::Eq, "  ").is_err());
    assert!(
        rule_from_text("n", FilterOp::Between, "5")
            .unwrap_err()
            .contains("two values")
    );
    assert!(
        rule_from_text("n", FilterOp::In, " , ")
            .unwrap_err()
            .contains("at least one")
    );
    assert!(rule_from_text("", FilterOp::Eq, "x").is_err());
}
