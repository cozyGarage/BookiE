use super::*;

#[test]
fn display_payloads_are_bounded_while_the_original_cell_keeps_exact_text_and_bytes() {
    for value in [
        Value::Text("💾".repeat(1_000_000)),
        Value::Text("\0".repeat(1_000_000)),
        Value::JsonText(format!("{{ \"payload\":  \"{}\" }}", "x".repeat(1_000_000))),
        Value::Bytes(vec![0xab; 1_000_000]),
        Value::Json(Box::new(serde_json::json!({"payload": "x".repeat(1_000_000)}))),
    ] {
        let display = display_cell(value.clone()).unwrap();
        assert!(serde_json::to_vec(&display).unwrap().len() < 4096);
        assert!(display.value.as_ref().unwrap().capacity() <= 8192);
        assert!(display.previewed);
        assert!(display.value.as_ref().unwrap().contains('…'));
        let original = cell(value).unwrap();
        assert!(!original.previewed);
        assert!(original.value.unwrap().len() > 4096);
    }
}

#[test]
fn exact_cell_requests_validate_coordinates_and_return_the_original_synthetic_value() {
    let exact = full_cell(9, 3, Some(150_000)).unwrap();
    assert_eq!(exact.kind, "json_text");
    assert_eq!(exact.value.as_deref(), Some("{\"raw\":  1,   \"keep\" : 2}"));
    assert!(!exact.previewed);
    assert!(full_cell(150_000, 0, Some(150_000)).is_err());
    assert!(full_cell(0, 4, None).is_err());
    assert!(full_cell(0, 0, Some(42)).is_err());
}

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
    assert!(!nulls.matches.is_empty());
    assert!(
        nulls.matches.iter().all(|row| row % TYPED_KINDS == 8),
        "only the typed JSON value contains the word null; NULL cells never match: {:?}",
        nulls.matches
    );
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

fn matching(total: u64, predicate: &Predicate) -> Vec<u64> {
    (0..total)
        .filter(|index| matches_predicate(*index, predicate))
        .collect()
}

fn rendered(value: Value) -> (&'static str, Option<String>) {
    let cell = cell(value).unwrap();
    (cell.kind, cell.value)
}

#[test]
fn every_core_value_variant_renders_with_its_exact_text() {
    use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
    let date = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
    let time = NaiveTime::from_hms_micro_opt(23, 59, 59, 123_456).unwrap();
    let cases: Vec<(Value, &str, Option<&str>)> = vec![
        (Value::Null, "null", None),
        (Value::Bool(false), "boolean", Some("false")),
        (Value::Int(i64::MAX), "integer", Some("9223372036854775807")),
        (Value::Float(0.1 + 0.2), "float", Some("0.30000000000000004")),
        (Value::Text("héllo".into()), "text", Some("héllo")),
        (Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]), "bytes", Some("\\xdeadbeef")),
        (Value::Date(date), "date", Some("2026-11-04")),
        (Value::Time(time), "time", Some("23:59:59.123456")),
        (
            Value::DateTime(date.and_time(time)),
            "datetime",
            Some("2026-11-04 23:59:59.123456"),
        ),
        (
            Value::TimestampTz(Utc.from_utc_datetime(&date.and_time(time))),
            "timestamptz",
            Some("2026-11-04T23:59:59.123456+00:00"),
        ),
        (
            Value::Decimal("12345678901234567890.12345678".parse().unwrap()),
            "decimal",
            Some("12345678901234567890.12345678"),
        ),
        (
            Value::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8".parse().unwrap()),
            "uuid",
            Some("67e55044-10b1-426f-9247-bb680e5fe0c8"),
        ),
        (
            Value::Json(Box::new(serde_json::json!({"a": [1, {"b": null}]}))),
            "json",
            Some(r#"{"a":[1,{"b":null}]}"#),
        ),
        (
            Value::JsonText("{\"raw\":  1,   \"keep\" : 2}".into()),
            "json_text",
            Some("{\"raw\":  1,   \"keep\" : 2}"),
        ),
        (
            Value::Undecodable("numeric(40,20)".into()),
            "undecodable",
            Some("<undecodable numeric(40,20)>"),
        ),
    ];
    for (value, kind, text) in cases {
        let label = format!("{value:?}");
        assert_eq!(rendered(value), (kind, text.map(str::to_string)), "{label}");
    }
}

#[test]
fn raw_json_text_keeps_its_spacing_while_parsed_json_does_not() {
    let raw = "{\"a\" :  1}";
    assert_eq!(rendered(Value::JsonText(raw.into())).1.as_deref(), Some(raw));
    let parsed = Value::Json(Box::new(serde_json::from_str(raw).unwrap()));
    assert_eq!(rendered(parsed).1.as_deref(), Some(r#"{"a":1}"#));
}

#[test]
fn rows_carry_a_fourth_typed_cell_that_cycles_through_every_variant() {
    let rows = sample_rows(0, TYPED_KINDS as usize, None).unwrap();
    assert!(rows.iter().all(|row| row.cells.len() == 4));
    let kinds: Vec<&str> = rows.iter().map(|row| row.cells[3].kind).collect();
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
    assert_eq!(rows[0].cells[0].value.as_deref(), Some("9007199254740993"));
}

#[test]
fn the_typed_fixture_shows_exactly_the_texts_the_gpui_prototype_pins() {
    let rows = sample_rows(0, TYPED_KINDS as usize, None).unwrap();
    let shown: Vec<Option<&str>> = rows.iter().map(|row| row.cells[3].value.as_deref()).collect();
    assert_eq!(
        shown,
        [
            Some("0.30000000000000004"),
            Some("\\xdeadbeef"),
            Some("2026-11-04"),
            Some("23:59:59.123456"),
            Some("2026-11-04 23:59:59.123456"),
            Some("2026-11-04T23:59:59.123456+00:00"),
            Some("12345678901234567890.12345678"),
            Some("67e55044-10b1-426f-9247-bb680e5fe0c8"),
            Some(r#"{"a":[1,{"b":null}]}"#),
            Some("{\"raw\":  1,   \"keep\" : 2}"),
            Some("<undecodable numeric(40,20)>"),
            None,
        ]
    );
}

#[test]
fn a_filter_and_find_can_target_the_typed_column() {
    let hits = matching(150_000, &predicate("typed", "contains", "deadbeef"));
    assert_eq!(hits.first().copied(), Some(1));
    assert!(hits.iter().all(|row| row % TYPED_KINDS == 1));
    let not_null = matching(24, &predicate("typed", "not_equals", "<undecodable numeric(40,20)>"));
    assert_eq!(not_null.len(), 22);
    let found = find_rows(Some(150_000), "numeric(40,20)".into(), 3, 1).unwrap();
    assert_eq!(found.matches, vec![10, 22, 34]);
    assert!(valid_predicate(&predicate("typed", "equals", "x")).is_ok());
}

#[test]
fn copying_rows_uses_the_product_tsv_with_exact_digits_and_the_null_marker() {
    let copied = copy_rows(0, 2, true, None).unwrap();
    let lines: Vec<&str> = copied.lines().collect();
    assert_eq!(lines[0], "value\tlabel\tstate\ttyped");
    assert_eq!(lines[1], "9007199254740993\tSample 0\t\\N\t0.30000000000000004");
    assert_eq!(lines[2], "9007199254740994\tSample 1\tfalse\t\\xdeadbeef");
    assert_eq!(lines[3], "9007199254740995\tSample 2\ttrue\t2026-11-04");
    assert_eq!(lines.len(), 4);
}

#[test]
fn copying_without_headers_and_every_typed_kind_stay_on_one_line_each() {
    let copied = copy_rows(0, TYPED_KINDS - 1, false, None).unwrap();
    assert_eq!(copied.lines().count(), TYPED_KINDS as usize);
    assert!(copied.lines().all(|line| line.matches('\t').count() == 3), "{copied}");
    assert!(copied.contains("<undecodable numeric(40,20)>"));
    assert!(
        copied.lines().last().unwrap().ends_with("\\N"),
        "the last typed kind is NULL"
    );
}

#[test]
fn copying_is_bounded_and_refuses_reversed_or_oversized_ranges() {
    assert!(copy_rows(5, 4, true, None).is_err());
    assert!(copy_rows(0, MAX_COPY_ROWS, true, None).is_err());
    assert!(copy_rows(0, MAX_COPY_ROWS - 1, true, None).is_ok());
    assert!(copy_rows(0, 1, true, Some(42)).is_err());
}

#[test]
fn copying_past_the_last_row_stops_at_the_end() {
    let copied = copy_rows(149_998, 150_005, false, Some(150_000)).unwrap();
    assert_eq!(copied.lines().count(), 2);
}

fn edited(kind: &str, text: &str) -> Result<(String, Option<String>), String> {
    edit_cell(kind.into(), text.into(), false).map(|cell| (cell.kind.to_string(), cell.value))
}

#[test]
fn an_edit_is_parsed_with_the_core_rules_and_shown_in_canonical_form() {
    assert_eq!(edited("integer", " 42 "), Ok(("integer".into(), Some("42".into()))));
    assert_eq!(edited("boolean", "Yes"), Ok(("boolean".into(), Some("true".into()))));
    assert_eq!(edited("decimal", "1.50"), Ok(("decimal".into(), Some("1.50".into()))));
    assert_eq!(
        edited("date", "2026-02-03"),
        Ok(("date".into(), Some("2026-02-03".into())))
    );
    assert_eq!(
        edited("uuid", "67E55044-10B1-426F-9247-BB680E5FE0C8"),
        Ok(("uuid".into(), Some("67e55044-10b1-426f-9247-bb680e5fe0c8".into())))
    );
    assert_eq!(
        edited("bytes", "0xdeadbeef"),
        Ok(("bytes".into(), Some("\\xdeadbeef".into())))
    );
    assert_eq!(
        edited("json", "{\"a\": 1}"),
        Ok(("json".into(), Some("{\"a\":1}".into())))
    );
    assert_eq!(
        edited("text", "  keep spaces "),
        Ok(("text".into(), Some("  keep spaces ".into())))
    );
}

#[test]
fn an_invalid_edit_names_the_problem_without_echoing_the_text() {
    for (kind, text, reason) in [
        ("integer", "4x2", "not a whole number"),
        ("boolean", "maybe", "not true or false"),
        ("decimal", "1,5", "not a number"),
        ("date", "2026-13-40", "not a date"),
        ("time", "25:00", "not a time"),
        ("timestamptz", "yesterday", "not a timestamp"),
        ("uuid", "abc", "not a UUID"),
        ("json", "{oops", "not JSON"),
        ("bytes", "xyz", "not hexadecimal bytes"),
    ] {
        let error = edited(kind, text).unwrap_err();
        assert_eq!(error, reason, "{kind}");
        assert!(!error.contains(text) || text.len() < 4, "{kind}");
    }
}

#[test]
fn null_and_read_only_cells_follow_their_own_rules() {
    assert_eq!(
        edit_cell("integer".into(), "ignored".into(), true).unwrap().kind,
        "null"
    );
    assert_eq!(
        edit_cell("undecodable".into(), "x".into(), false).unwrap_err(),
        "this cell cannot be edited"
    );
    assert_eq!(edited("null", "now text"), Ok(("text".into(), Some("now text".into()))));
    assert_eq!(
        edited("json_text", "{\"raw\":  1}"),
        Ok(("json_text".into(), Some("{\"raw\":  1}".into())))
    );
}
