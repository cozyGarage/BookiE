use super::model::*;
use super::source::{ResultSource, Synthetic};
use tokio_util::sync::CancellationToken;

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
    let found = Synthetic::new(150_000)
        .find("sample 14999", 10_000, &CancellationToken::new())
        .unwrap();
    assert_eq!(found.matches.first(), Some(&14_999));
    assert!(found.matches.contains(&149_990));
    assert!(!found.capped);
    assert_eq!(found.scanned, 150_000);
}

#[test]
fn find_stops_at_the_cap_and_says_so() {
    let found = Synthetic::new(150_000)
        .find("sample", 100, &CancellationToken::new())
        .unwrap();
    assert_eq!(found.matches.len(), 100);
    assert!(found.capped);
    assert_eq!(found.scanned, 101);
}

#[test]
fn find_refuses_bad_requests() {
    assert!(Synthetic::new(10).find("", 5, &CancellationToken::new()).is_err());
    assert!(Synthetic::new(10).find("x", 0, &CancellationToken::new()).is_err());
    assert!(
        Synthetic::new(10)
            .find("x", MAX_FIND_MATCHES + 1, &CancellationToken::new())
            .is_err()
    );
    assert!(
        Synthetic::new(10)
            .find(&"x".repeat(257), 5, &CancellationToken::new())
            .is_err()
    );
}

#[test]
fn cancelled_scans_return_and_a_cancelled_initialization_can_be_retried() {
    let source = Synthetic::new(10);
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(source.find("sample", 10, &cancel), Err("the operation was cancelled"));
    let predicate = parse_predicate("label contains 1", &synthetic_columns()).unwrap();
    assert_eq!(source.filter(&predicate, &cancel), Err("the operation was cancelled"));
    assert!(matches!(source.compare(false, &cancel), Err("comparison cancelled")));
    assert_eq!(
        source.find("sample 1", 10, &CancellationToken::new()).unwrap().matches,
        vec![1]
    );
}

#[test]
fn null_never_matches_text_but_not_equals_keeps_it() {
    let source = Synthetic::new(6);
    let equals = parse_predicate("state = true", &synthetic_columns()).unwrap();
    assert_eq!(source.filter(&equals, &CancellationToken::new()), Ok(vec![2, 4]));
    let not_equals = parse_predicate("state != true", &synthetic_columns()).unwrap();
    assert_eq!(
        source.filter(&not_equals, &CancellationToken::new()),
        Ok(vec![0, 1, 3, 5])
    );
}

#[test]
fn filtering_keeps_the_exact_typed_integer_text() {
    let predicate = parse_predicate("value != 9007199254740993", &synthetic_columns()).unwrap();
    let kept = Synthetic::new(10)
        .filter(&predicate, &CancellationToken::new())
        .unwrap();
    assert_eq!(kept.len(), 9);
    assert_eq!(kept[0], 1);
}

#[test]
fn filtering_typed_values_matches_their_displayed_text() {
    let source = Synthetic::new(30);
    let cases = [
        ("0.30000000000000004", 0),
        (r"\xdeadbeef", 1),
        ("2026-11-04", 2),
        ("23:59:59.123456", 3),
        ("2026-11-04 23:59:59.123456", 4),
        ("2026-11-04T23:59:59.123456+00:00", 5),
        ("12345678901234567890.12345678", 6),
        ("67e55044-10b1-426f-9247-bb680e5fe0c8", 7),
        ("67e55044-10b1-426f-9247-bb680e5fe0c8", 7),
        (r#"{"a":[1,{"b":null}]}"#, 8),
        (r#"{"raw":  1,   "keep" : 2}"#, 9),
        ("<undecodable numeric(40,20)>", 10),
    ];
    for (text, expected_modulo) in cases {
        let predicate = parse_predicate(&format!("typed = {text}"), &synthetic_columns()).unwrap();
        let expected = (0..30)
            .filter(|index| index % 12 == expected_modulo)
            .collect::<Vec<_>>();
        assert_eq!(
            source.filter(&predicate, &CancellationToken::new()),
            Ok(expected),
            "{text}"
        );
    }

    let not_equals = parse_predicate("typed != 2026-11-04", &synthetic_columns()).unwrap();
    assert_eq!(
        source.filter(&not_equals, &CancellationToken::new()).unwrap(),
        (0..30).filter(|index| ![2, 14, 26].contains(index)).collect::<Vec<_>>()
    );
}

#[test]
fn filtering_text_values_is_exact_and_case_sensitive() {
    let exact = parse_predicate("label = Sample 1", &synthetic_columns()).unwrap();
    assert_eq!(
        Synthetic::new(10).filter(&exact, &CancellationToken::new()),
        Ok(vec![1])
    );
    let folded = parse_predicate("label = sample 1", &synthetic_columns()).unwrap();
    assert_eq!(
        Synthetic::new(10).filter(&folded, &CancellationToken::new()),
        Ok(vec![])
    );
}

#[test]
fn predicates_are_validated() {
    assert!(parse_predicate("label contains 7", &synthetic_columns()).is_ok());
    assert!(parse_predicate("nope contains 7", &synthetic_columns()).is_err());
    assert!(parse_predicate("label like 7", &synthetic_columns()).is_err());
    assert!(parse_predicate("label contains", &synthetic_columns()).is_err());
    assert!(parse_predicate(&format!("label contains {}", "x".repeat(257)), &synthetic_columns()).is_err());
}

#[test]
fn whole_row_compare_counts_added_and_removed_pairs() {
    let diff = Synthetic::new(1_000).compare(false, &CancellationToken::new()).unwrap();
    assert!(!diff.summary.keyed);
    assert_eq!(diff.summary.changed, 0);
    assert!(diff.summary.added > 0 && diff.summary.removed > 0);
    assert_eq!(diff.rows.len().min(MAX_DIFF_ROWS), diff.rows.len());
}

#[test]
fn keyed_compare_pairs_before_and_after_for_changed_rows() {
    let diff = Synthetic::new(1_000).compare(true, &CancellationToken::new()).unwrap();
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
    let diff = Synthetic::new(150_000)
        .compare(false, &CancellationToken::new())
        .unwrap();
    assert_eq!(diff.rows.len(), MAX_DIFF_ROWS);
    assert!(diff.capped);
    assert!(diff.summary.removed > 0 && diff.summary.added > 0);
    assert!(diff.summary.unknown > 0, "undecodable rows are unknown, not equal");
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
    assert_eq!(pager.range(), (0, 0));
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

#[test]
fn the_typed_column_cycles_through_the_same_kinds_as_the_tauri_prototype() {
    let kinds: Vec<&str> = (0..TYPED_KINDS).map(|index| typed_of(index).kind()).collect();
    assert_eq!(
        kinds,
        [
            "float",
            "bytes",
            "date",
            "time",
            "datetime",
            "timestamptz",
            "decimal",
            "uuid",
            "json",
            "json_text",
            "undecodable",
            "null"
        ]
    );
    assert_eq!(typed_of(TYPED_KINDS).kind(), "float");
    assert!(row_at(7).len() == 4 && row_at(7)[3] == typed_of(7));
}

#[test]
fn every_kind_displays_the_same_exact_text_as_the_tauri_prototype() {
    let shown: Vec<String> = (0..TYPED_KINDS).map(|index| typed_of(index).display()).collect();
    assert_eq!(
        shown,
        [
            "0.30000000000000004",
            "\\xdeadbeef",
            "2026-11-04",
            "23:59:59.123456",
            "2026-11-04 23:59:59.123456",
            "2026-11-04T23:59:59.123456+00:00",
            "12345678901234567890.12345678",
            "67e55044-10b1-426f-9247-bb680e5fe0c8",
            r#"{"a":[1,{"b":null}]}"#,
            "{\"raw\":  1,   \"keep\" : 2}",
            "<undecodable numeric(40,20)>",
            "NULL"
        ]
    );
}

#[test]
fn bytes_float_and_undecodable_render_by_their_own_rules() {
    assert_eq!(Cell::Bytes(vec![]).display(), "\\x");
    assert_eq!(Cell::Bytes(vec![0x00, 0xff]).display(), "\\x00ff");
    assert_eq!(Cell::Float(1.0).display(), "1");
    assert_eq!(Cell::Undecodable("geometry".into()).display(), "<undecodable geometry>");
}

#[test]
fn a_filter_and_find_can_target_the_typed_column() {
    let predicate = parse_predicate("typed contains deadbeef", &synthetic_columns()).unwrap();
    let hits = Synthetic::new(150_000)
        .filter(&predicate, &CancellationToken::new())
        .unwrap();
    assert_eq!(hits.first().copied(), Some(1));
    assert!(hits.iter().all(|row| u64::from(*row) % TYPED_KINDS == 1));
    let not_undecodable = parse_predicate("typed != <undecodable numeric(40,20)>", &synthetic_columns()).unwrap();
    assert_eq!(
        Synthetic::new(24)
            .filter(&not_undecodable, &CancellationToken::new())
            .map(|kept| kept.len()),
        Ok(22)
    );
    let found = Synthetic::new(150_000)
        .find("numeric(40,20)", 3, &CancellationToken::new())
        .unwrap();
    assert!(found.matches.is_empty(), "undecodable cells are not searchable");
}

#[test]
fn only_the_typed_json_value_contains_the_word_null_and_a_null_cell_never_matches() {
    let found = Synthetic::new(150_000)
        .find("null", 10, &CancellationToken::new())
        .unwrap();
    assert!(!found.matches.is_empty());
    assert!(
        found.matches.iter().all(|row| row % TYPED_KINDS == 8),
        "{:?}",
        found.matches
    );
}

#[test]
fn load_more_grows_the_window_from_the_first_row() {
    let mut pager = Pager::new(12_000);
    pager.append = true;
    assert_eq!(pager.window(), (0, 5_000));
    pager.next();
    assert_eq!(pager.window(), (0, 10_000));
    pager.next();
    assert_eq!(pager.range(), (1, 12_000));
}

#[test]
fn resizing_keeps_the_first_row_in_view() {
    let mut pager = Pager::new(12_000);
    pager.next();
    let resized = pager.resized(1_000);
    assert_eq!(resized.page, 5);
    assert_eq!(resized.window(), (5_000, 6_000));
    assert_eq!(resized.with_total(3_000).page, 0);
}
