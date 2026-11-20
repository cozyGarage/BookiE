use bookie_core::{FilterOp, FilterRule, FilterSet, FilterValue};

use super::{SimpleOp, simple_filter_parts, simple_filter_set};

#[test]
fn a_one_rule_filter_becomes_a_structured_set_with_the_matching_operator() {
    let cases = [
        (SimpleOp::Contains, FilterOp::Contains),
        (SimpleOp::Eq, FilterOp::Eq),
        (SimpleOp::NotEq, FilterOp::NotEq),
    ];
    for (simple, expected) in cases {
        let set = simple_filter_set("customer", simple, "Ada");
        assert_eq!(set.rules.len(), 1);
        assert_eq!(set.rules[0].column, "customer");
        assert_eq!(set.rules[0].op, expected);
        assert_eq!(set.rules[0].value, Some(FilterValue::Single("Ada".into())));
        assert_eq!(set.extra_sql, None);
    }
}

#[test]
fn a_stored_set_round_trips_back_to_its_parts() {
    for simple in [SimpleOp::Contains, SimpleOp::Eq, SimpleOp::NotEq] {
        let set = simple_filter_set("total", simple, "7");
        assert_eq!(simple_filter_parts(&set), Some(("total".into(), simple, "7".into())));
    }
}

#[test]
fn a_set_the_simple_form_cannot_express_is_not_converted() {
    let two_rules = FilterSet {
        rules: vec![
            simple_filter_set("a", SimpleOp::Eq, "1").rules.remove(0),
            simple_filter_set("b", SimpleOp::Eq, "2").rules.remove(0),
        ],
        ..FilterSet::default()
    };
    assert_eq!(simple_filter_parts(&two_rules), None);
    let between = FilterSet {
        rules: vec![FilterRule {
            column: "a".into(),
            op: FilterOp::Between,
            value: Some(FilterValue::Pair("1".into(), "2".into())),
        }],
        ..FilterSet::default()
    };
    assert_eq!(simple_filter_parts(&between), None);
    let is_null = FilterSet {
        rules: vec![FilterRule {
            column: "a".into(),
            op: FilterOp::IsNull,
            value: None,
        }],
        ..FilterSet::default()
    };
    assert_eq!(simple_filter_parts(&is_null), None);
    let raw = FilterSet {
        extra_sql: Some("a > 1".into()),
        ..simple_filter_set("a", SimpleOp::Eq, "1")
    };
    assert_eq!(simple_filter_parts(&raw), None);
    assert_eq!(simple_filter_parts(&FilterSet::default()), None);
}
