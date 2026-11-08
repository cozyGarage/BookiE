use bookie_core::FilterValue;

use super::{
    DisplayCell, HighlightError, HighlightOp, HighlightPaint, HighlightRule, HighlightRuleSet, HighlightTarget,
    paint_for_cell, prepare_rules,
};

fn rule(
    column: &str,
    op: HighlightOp,
    value: Option<FilterValue>,
    color: &str,
    target: HighlightTarget,
) -> HighlightRule {
    HighlightRule {
        column: column.into(),
        op,
        value,
        color: color.into(),
        target,
    }
}

fn prepared(rules: &[HighlightRule], columns: &[&str]) -> Vec<super::PreparedHighlightRule> {
    prepare_rules(rules, columns).expect("prepare")
}

fn cell(text: &str) -> DisplayCell<'_> {
    DisplayCell { text, is_null: false }
}

fn null_cell() -> DisplayCell<'static> {
    DisplayCell {
        text: "NULL",
        is_null: true,
    }
}

#[test]
fn contains_matches_displayed_text_without_touching_values() {
    let rules = prepared(
        &[rule(
            "status",
            HighlightOp::Contains,
            Some(FilterValue::Single("err".into())),
            "red",
            HighlightTarget::Cell,
        )],
        &["status"],
    );
    let painted = "ERROR";
    let paint = paint_for_cell(&rules, 0, cell(painted), |_| None::<(String, bool)>);
    assert_eq!(
        paint,
        Some(HighlightPaint {
            color: "red",
            target: HighlightTarget::Cell,
        })
    );
    assert_eq!(painted, "ERROR");
}

#[test]
fn equals_uses_exact_display_text_not_numeric_coercion() {
    let rules = prepared(
        &[rule(
            "code",
            HighlightOp::Eq,
            Some(FilterValue::Single("042".into())),
            "blue",
            HighlightTarget::Cell,
        )],
        &["code"],
    );
    assert!(paint_for_cell(&rules, 0, cell("042"), |_| None::<(String, bool)>).is_some());
    assert!(paint_for_cell(&rules, 0, cell("42"), |_| None::<(String, bool)>).is_none());
}

#[test]
fn empty_and_null_are_distinct_on_display() {
    let empty = prepared(
        &[rule(
            "note",
            HighlightOp::IsEmpty,
            None,
            "yellow",
            HighlightTarget::Cell,
        )],
        &["note"],
    );
    let nulls = prepared(
        &[rule("note", HighlightOp::IsNull, None, "orange", HighlightTarget::Cell)],
        &["note"],
    );
    assert!(paint_for_cell(&empty, 0, cell(""), |_| None::<(String, bool)>).is_some());
    assert!(paint_for_cell(&empty, 0, null_cell(), |_| None::<(String, bool)>).is_none());
    assert!(paint_for_cell(&nulls, 0, null_cell(), |_| None::<(String, bool)>).is_some());
    assert!(paint_for_cell(&nulls, 0, cell(""), |_| None::<(String, bool)>).is_none());
}

#[test]
fn between_compares_numbers_without_rounding_display_text() {
    let rules = prepared(
        &[rule(
            "amount",
            HighlightOp::Between,
            Some(FilterValue::Pair("1.0".into(), "2.0".into())),
            "green",
            HighlightTarget::Cell,
        )],
        &["amount"],
    );
    assert!(paint_for_cell(&rules, 0, cell("1.50"), |_| None::<(String, bool)>).is_some());
    assert!(paint_for_cell(&rules, 0, cell("2.000"), |_| None::<(String, bool)>).is_some());
    assert!(paint_for_cell(&rules, 0, cell("2.1"), |_| None::<(String, bool)>).is_none());
}

#[test]
fn cell_rule_beats_earlier_row_rule() {
    let rules = prepared(
        &[
            rule(
                "name",
                HighlightOp::Contains,
                Some(FilterValue::Single("a".into())),
                "blue",
                HighlightTarget::Row,
            ),
            rule(
                "status",
                HighlightOp::Eq,
                Some(FilterValue::Single("bad".into())),
                "red",
                HighlightTarget::Cell,
            ),
        ],
        &["name", "status"],
    );
    let row = [("alice", false), ("bad", false)];
    let paint = paint_for_cell(&rules, 1, cell(row[1].0), |idx| {
        row.get(idx).map(|(text, is_null)| ((*text).to_string(), *is_null))
    });
    assert_eq!(
        paint,
        Some(HighlightPaint {
            color: "red",
            target: HighlightTarget::Cell,
        })
    );
}

#[test]
fn first_matching_cell_rule_wins() {
    let rules = prepared(
        &[
            rule(
                "status",
                HighlightOp::Contains,
                Some(FilterValue::Single("a".into())),
                "teal",
                HighlightTarget::Cell,
            ),
            rule(
                "status",
                HighlightOp::Contains,
                Some(FilterValue::Single("a".into())),
                "red",
                HighlightTarget::Cell,
            ),
        ],
        &["status"],
    );
    assert_eq!(
        paint_for_cell(&rules, 0, cell("bad"), |_| None::<(String, bool)>).map(|paint| paint.color),
        Some("teal")
    );
}

#[test]
fn row_rule_paints_other_columns_from_matched_column_text() {
    let rules = prepared(
        &[rule(
            "status",
            HighlightOp::Eq,
            Some(FilterValue::Single("bad".into())),
            "purple",
            HighlightTarget::Row,
        )],
        &["name", "status"],
    );
    let row = [("alice", false), ("bad", false)];
    let paint = paint_for_cell(&rules, 0, cell(row[0].0), |idx| {
        row.get(idx).map(|(text, is_null)| ((*text).to_string(), *is_null))
    });
    assert_eq!(
        paint,
        Some(HighlightPaint {
            color: "purple",
            target: HighlightTarget::Row,
        })
    );
}

#[test]
fn unknown_column_is_skipped_and_unknown_colour_is_rejected() {
    let skipped = prepare_rules(
        &[rule(
            "missing",
            HighlightOp::IsEmpty,
            None,
            "red",
            HighlightTarget::Cell,
        )],
        &["status"],
    )
    .expect("unknown columns are skipped");
    assert!(skipped.is_empty());
    let err = prepare_rules(
        &[rule(
            "status",
            HighlightOp::IsEmpty,
            None,
            "chartreuse",
            HighlightTarget::Cell,
        )],
        &["status"],
    );
    assert_eq!(err, Err(HighlightError::UnknownColor));
}

#[test]
fn empty_rule_set_serializes_round_trip() {
    let set = HighlightRuleSet {
        rules: vec![rule(
            "status",
            HighlightOp::Eq,
            Some(FilterValue::Single("ok".into())),
            "green",
            HighlightTarget::Cell,
        )],
    };
    let json = serde_json::to_string(&set).expect("serialize");
    let loaded: HighlightRuleSet = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(loaded, set);
    assert!(!loaded.is_empty());
}

#[test]
fn no_match_returns_none() {
    let rules = prepared(
        &[rule(
            "status",
            HighlightOp::Eq,
            Some(FilterValue::Single("bad".into())),
            "red",
            HighlightTarget::Cell,
        )],
        &["status"],
    );
    assert_eq!(paint_for_cell(&rules, 0, cell("ok"), |_| None::<(String, bool)>), None);
}
