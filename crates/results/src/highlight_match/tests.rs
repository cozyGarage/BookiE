use bookie_core::FilterValue;

use super::{
    DisplayCell, HighlightError, HighlightOp, HighlightPaint, HighlightRule, HighlightRuleSet, HighlightTarget,
    MAX_LIKE_PATTERN_BYTES, compute_row_rule_matches, paint_for_cell, paint_for_cell_with_row_matches, prepare_rules,
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

#[test]
fn row_rule_matches_reuse_each_source_column_and_cell_value_remains_current() {
    let rules = prepared(
        &[
            rule(
                "status",
                HighlightOp::Eq,
                Some(FilterValue::Single("bad".into())),
                "red",
                HighlightTarget::Row,
            ),
            rule(
                "status",
                HighlightOp::Contains,
                Some(FilterValue::Single("b".into())),
                "blue",
                HighlightTarget::Row,
            ),
        ],
        &["status", "name"],
    );
    let mut reads = 0;
    let matches = compute_row_rule_matches(&rules, |index| {
        reads += 1;
        (index == 0).then(|| (String::from("bad"), false))
    });

    assert_eq!(reads, 1);
    assert_eq!(matches, 0b11);
    assert_eq!(
        paint_for_cell_with_row_matches(&rules, 1, cell("name"), matches),
        Some(HighlightPaint {
            color: "red",
            target: HighlightTarget::Row
        })
    );
    assert_eq!(
        paint_for_cell_with_row_matches(&rules, 0, cell("bingo"), matches),
        Some(HighlightPaint {
            color: "blue",
            target: HighlightTarget::Row
        })
    );
}

#[test]
fn row_rules_after_the_inline_cache_capacity_still_match() {
    let mut source_rules = (0..u64::BITS)
        .map(|_| {
            rule(
                "name",
                HighlightOp::Eq,
                Some(FilterValue::Single("never".into())),
                "blue",
                HighlightTarget::Cell,
            )
        })
        .collect::<Vec<_>>();
    source_rules.push(rule(
        "status",
        HighlightOp::Eq,
        Some(FilterValue::Single("bad".into())),
        "red",
        HighlightTarget::Row,
    ));
    let rules = prepared(&source_rules, &["status", "name"]);

    assert_eq!(
        paint_for_cell(&rules, 1, cell("name"), |index| {
            (index == 0).then(|| (String::from("bad"), false))
        }),
        Some(HighlightPaint {
            color: "red",
            target: HighlightTarget::Row
        })
    );
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
fn like_no_match_with_many_wildcard_segments_finishes() {
    let pattern = format!("{}b", "%a".repeat(24));
    let text = format!("{}c", "a".repeat(128));
    let rules = prepared(
        &[rule(
            "value",
            HighlightOp::Like,
            Some(FilterValue::Single(pattern)),
            "red",
            HighlightTarget::Cell,
        )],
        &["value"],
    );

    assert_eq!(paint_for_cell(&rules, 0, cell(&text), |_| None::<(String, bool)>), None);
}

#[test]
fn like_wildcards_match_unicode_characters_and_newlines() {
    let rules = prepared(
        &[rule(
            "value",
            HighlightOp::Like,
            Some(FilterValue::Single("ab_%z".into())),
            "red",
            HighlightTarget::Cell,
        )],
        &["value"],
    );

    assert!(paint_for_cell(&rules, 0, cell("abé\nz"), |_| None::<(String, bool)>).is_some());
    assert!(paint_for_cell(&rules, 0, cell("abz"), |_| None::<(String, bool)>).is_none());
}

#[test]
fn like_treats_regex_metacharacters_as_literal_text() {
    let rules = prepared(
        &[rule(
            "value",
            HighlightOp::Like,
            Some(FilterValue::Single("a.+%".into())),
            "red",
            HighlightTarget::Cell,
        )],
        &["value"],
    );

    assert!(paint_for_cell(&rules, 0, cell("a.+tail"), |_| None::<(String, bool)>).is_some());
    assert!(paint_for_cell(&rules, 0, cell("abbbtail"), |_| None::<(String, bool)>).is_none());
}

#[test]
fn ilike_preserves_unicode_lowercase_expansion_and_not_like_negates_matches() {
    let ilike = prepared(
        &[rule(
            "value",
            HighlightOp::Ilike,
            Some(FilterValue::Single("İ%".into())),
            "blue",
            HighlightTarget::Cell,
        )],
        &["value"],
    );
    let not_like = prepared(
        &[rule(
            "value",
            HighlightOp::NotLike,
            Some(FilterValue::Single("alpha%".into())),
            "green",
            HighlightTarget::Cell,
        )],
        &["value"],
    );

    assert!(paint_for_cell(&ilike, 0, cell("İREADY"), |_| None::<(String, bool)>).is_some());
    assert!(paint_for_cell(&not_like, 0, cell("beta"), |_| None::<(String, bool)>).is_some());
    assert!(paint_for_cell(&not_like, 0, cell("alpha"), |_| None::<(String, bool)>).is_none());
}

#[test]
fn like_pattern_over_the_byte_limit_is_rejected() {
    let pattern = "é".repeat(MAX_LIKE_PATTERN_BYTES / 2 + 1);
    let result = prepare_rules(
        &[rule(
            "value",
            HighlightOp::Like,
            Some(FilterValue::Single(pattern)),
            "red",
            HighlightTarget::Cell,
        )],
        &["value"],
    );

    assert_eq!(result, Err(HighlightError::PatternTooLong));
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
