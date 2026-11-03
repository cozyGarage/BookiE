use super::*;

fn predicate(column: &str, op: &str, text: &str) -> Predicate {
    Predicate {
        column: column.into(),
        op: op.into(),
        text: text.into(),
    }
}

#[test]
fn rows_are_bounded_and_large_integers_keep_all_digits() {
    let rows = sample_rows(0, 2, None).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].cells[0].value.as_deref(), Some("9007199254740993"));
    assert_eq!(rows[0].cells[2].kind, "null");
    assert!(sample_rows(0, MAX_BATCH + 1, None).is_err());
    assert_eq!(sample_rows(999_999, 10, None).unwrap().len(), 1);
    assert!(sample_rows(u64::MAX, 10, None).unwrap().is_empty());
}

#[test]
fn only_the_listed_synthetic_sizes_are_accepted() {
    assert_eq!(sample_rows(149_999, 10, Some(150_000)).unwrap().len(), 1);
    assert!(sample_rows(0, 1, Some(42)).is_err());
    assert!(find_rows(Some(7), "a".into(), 10, 1).is_err());
}

#[test]
fn find_echoes_the_generation_and_reports_exact_matches() {
    let reply = find_rows(Some(150_000), "sample 149999".into(), 100, 7).unwrap();
    assert_eq!(reply.generation, 7);
    assert_eq!(reply.matches, vec![149_999]);
    assert!(!reply.capped);
    assert_eq!(reply.scanned, 150_000);
}

#[test]
fn find_is_capped_and_refuses_larger_caps() {
    let reply = find_rows(Some(150_000), "sample".into(), 25, 1).unwrap();
    assert_eq!(reply.matches.len(), 25);
    assert!(reply.capped);
    assert!(find_rows(Some(150_000), "sample".into(), MAX_FIND_MATCHES + 1, 1).is_err());
    assert!(find_rows(Some(150_000), "sample".into(), 0, 1).is_err());
    assert!(find_rows(Some(150_000), String::new(), 5, 1).is_err());
}

#[test]
fn find_matches_exact_integer_text_and_never_matches_null() {
    let reply = find_rows(Some(150_000), "9007199254740993".into(), 10, 1).unwrap();
    assert_eq!(reply.matches, vec![0]);
    let nulls = find_rows(Some(150_000), "null".into(), 10, 1).unwrap();
    assert!(nulls.matches.is_empty());
}

#[test]
fn predicates_keep_null_state_distinct() {
    assert!(matches_predicate(1, &predicate("state", "equals", "false")));
    assert!(!matches_predicate(0, &predicate("state", "equals", "false")));
    assert!(matches_predicate(0, &predicate("state", "not_equals", "false")));
    assert!(!matches_predicate(0, &predicate("state", "contains", "null")));
}

#[test]
fn invalid_predicates_are_refused() {
    assert!(valid_predicate(&predicate("nope", "equals", "x")).is_err());
    assert!(valid_predicate(&predicate("label", "regex", "x")).is_err());
    assert!(valid_predicate(&predicate("label", "equals", "")).is_err());
    assert!(valid_predicate(&predicate("label", "equals", "x")).is_ok());
}

#[test]
fn whole_row_diff_counts_added_removed_and_equal_rows() {
    let reply = build_diff(150_000, false, 3).unwrap();
    assert_eq!(reply.generation, 3);
    assert_eq!(
        reply.summary,
        DiffSummary {
            mode: "whole_row",
            equal: 147_000,
            added: 1_500,
            removed: 3_000,
            changed: 0
        }
    );
    assert_eq!(reply.rows.len(), MAX_DIFF_ROWS);
    assert!(reply.capped);
}

#[test]
fn keyed_diff_reports_changed_rows_as_before_and_after_pairs() {
    let reply = build_diff(150_000, true, 4).unwrap();
    assert_eq!(
        reply.summary,
        DiffSummary {
            mode: "keyed",
            equal: 147_000,
            added: 0,
            removed: 1_500,
            changed: 1_500
        }
    );
    let first = reply.rows.iter().position(|row| row.change == "Before").unwrap();
    assert_eq!(reply.rows[first + 1].change, "After");
    assert_eq!(reply.rows[first].cells[0].value, reply.rows[first + 1].cells[0].value);
}
