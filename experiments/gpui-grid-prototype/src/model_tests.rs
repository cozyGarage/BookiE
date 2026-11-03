use super::model::*;

#[test]
fn row_count_selection_accepts_only_the_three_sizes() {
    assert_eq!(rows_from(Some("150000")), 150_000);
    assert_eq!(rows_from(Some("500000")), 500_000);
    assert_eq!(rows_from(Some("1000000")), 1_000_000);
    assert_eq!(rows_from(Some("7")), DEFAULT_ROWS);
    assert_eq!(rows_from(Some("many")), DEFAULT_ROWS);
    assert_eq!(rows_from(None), DEFAULT_ROWS);
}

#[test]
fn the_generator_matches_the_tauri_prototype() {
    assert_eq!(value_of(0), 9_007_199_254_740_993);
    assert_eq!(value_of(5), 9_007_199_254_740_998);
    assert_eq!(label_of(41), "Sample 41");
    assert_eq!(state_of(0), Cell::Null);
    assert_eq!(state_of(1), Cell::Bool(false));
    assert_eq!(state_of(2), Cell::Bool(true));
    assert_eq!(state_of(3), Cell::Null);
    let row = row_at(2);
    assert_eq!(row[0], Cell::Int(9_007_199_254_740_995));
    assert_eq!(row[1], Cell::Text("Sample 2".into()));
    assert_eq!(row[2], Cell::Bool(true));
}

#[test]
fn integers_beyond_the_float_range_keep_every_digit() {
    assert_eq!(Cell::Int(value_of(0)).display(), "9007199254740993");
    assert_ne!(value_of(0) as f64 as i64, value_of(0));
}

#[test]
fn find_matches_any_column_case_insensitively_and_reports_the_scan() {
    let found = find_rows(150_000, "sample 14999", 10_000).unwrap();
    assert_eq!(found.matches.first(), Some(&14_999));
    assert!(found.matches.contains(&149_990));
    assert!(!found.capped);
    assert_eq!(found.scanned, 150_000);
}

#[test]
fn find_stops_at_the_cap_and_says_so() {
    let found = find_rows(150_000, "sample", 100).unwrap();
    assert_eq!(found.matches.len(), 100);
    assert!(found.capped);
    assert_eq!(found.scanned, 100);
}

#[test]
fn find_refuses_bad_requests() {
    assert!(find_rows(10, "", 5).is_err());
    assert!(find_rows(10, "x", 0).is_err());
    assert!(find_rows(10, "x", MAX_FIND_MATCHES + 1).is_err());
    assert!(find_rows(10, &"x".repeat(257), 5).is_err());
}

#[test]
fn null_never_matches_text_but_not_equals_keeps_it() {
    let equals = parse_predicate("state = true").unwrap();
    assert!(matches_predicate(2, &equals));
    assert!(!matches_predicate(0, &equals));
    let not_equals = parse_predicate("state != true").unwrap();
    assert!(matches_predicate(0, &not_equals));
    assert!(matches_predicate(1, &not_equals));
    assert!(!matches_predicate(2, &not_equals));
}

#[test]
fn filtering_keeps_the_exact_typed_integer_text() {
    let predicate = parse_predicate("value != 9007199254740993").unwrap();
    assert!(!matches_predicate(0, &predicate));
    assert!(matches_predicate(1, &predicate));
    let kept = filter_rows(10, &predicate);
    assert_eq!(kept.len(), 9);
    assert_eq!(kept[0], 1);
}

#[test]
fn predicates_are_validated() {
    assert!(parse_predicate("label contains 7").is_ok());
    assert!(parse_predicate("nope contains 7").is_err());
    assert!(parse_predicate("label like 7").is_err());
    assert!(parse_predicate("label contains").is_err());
    assert!(parse_predicate(&format!("label contains {}", "x".repeat(257))).is_err());
}

#[test]
fn whole_row_compare_counts_added_and_removed_pairs() {
    let diff = build_diff(1_000, false);
    assert!(!diff.summary.keyed);
    assert_eq!(diff.summary.changed, 0);
    let dropped = 10;
    let edited = 10;
    assert_eq!(diff.summary.removed, dropped + edited);
    assert_eq!(diff.summary.added, edited);
    assert_eq!(diff.summary.equal, 1_000 - dropped - edited);
    assert_eq!(diff.rows.len().min(MAX_DIFF_ROWS), diff.rows.len());
}

#[test]
fn keyed_compare_pairs_before_and_after_for_changed_rows() {
    let diff = build_diff(1_000, true);
    assert!(diff.summary.keyed);
    assert_eq!(diff.summary.changed, 10);
    assert_eq!(diff.summary.added, 0);
    assert_eq!(diff.summary.removed, 10);
    let changes: Vec<&str> = diff.rows.iter().map(|row| row.change).collect();
    let before = changes.iter().position(|change| *change == "Before").unwrap();
    assert_eq!(changes[before + 1], "After");
    assert_eq!(diff.rows[before + 1].cells[1], Cell::Text("Sample 1 (edited)".into()));
}

#[test]
fn compare_caps_the_rows_it_shows_but_keeps_exact_counts() {
    let diff = build_diff(150_000, false);
    assert_eq!(diff.rows.len(), MAX_DIFF_ROWS);
    assert!(diff.capped);
    assert_eq!(diff.summary.removed, 1_500 + 1_500);
    assert_eq!(diff.summary.added, 1_500);
    assert_eq!(diff.summary.equal, 150_000 - 3_000);
}

#[test]
fn the_pager_clamps_and_reports_the_visible_range() {
    let mut pager = Pager::new(12_000);
    assert_eq!(pager.pages(), 3);
    assert_eq!(pager.range(), (1, 5_000));
    pager.next();
    assert_eq!(pager.range(), (5_001, 10_000));
    pager.last();
    assert_eq!(pager.range(), (10_001, 12_000));
    pager.next();
    assert_eq!(pager.page, 2);
    pager.first();
    pager.previous();
    assert_eq!(pager.page, 0);
    pager.contains_row(11_999);
    assert_eq!(pager.page, 2);
    pager.contains_row(u64::MAX);
    assert_eq!(pager.page, 2);
}

#[test]
fn an_empty_result_has_one_empty_page() {
    let pager = Pager::new(0);
    assert_eq!(pager.pages(), 1);
}

#[test]
fn the_match_cursor_wraps_both_ways_and_names_its_position() {
    let mut cursor = MatchCursor::default();
    assert_eq!(cursor.status(false), "No match");
    assert_eq!(cursor.step(true), None);
    cursor.set(vec![10, 20, 30]);
    assert_eq!(cursor.status(false), "Match 0 of 3");
    assert_eq!(cursor.step(true), Some(10));
    assert_eq!(cursor.status(false), "Match 1 of 3");
    assert_eq!(cursor.step(false), Some(30));
    assert_eq!(cursor.step(true), Some(10));
    assert_eq!(cursor.status(true), "Match 1 of 3+");
    cursor.set(vec![5]);
    assert_eq!(cursor.step(false), Some(5));
}
