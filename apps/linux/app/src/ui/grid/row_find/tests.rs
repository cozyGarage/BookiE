use std::io::Write;

use bookie_app_services::text_match::TextMatchOptions;
use bookie_core::Value;
use tokio_util::sync::CancellationToken;

use super::super::display::value_to_display_text;
use super::dialog::preview;
use super::json_search::{CaseInsensitivePattern, JsonSearchWriter, matches_value};
use super::query::{FindQuery, matching_rows};
use super::replace::plan_replacements;

const PREVIEW_CHARS: usize = 80;

struct SerializationReached(std::cell::Cell<bool>);

impl serde::Serialize for SerializationReached {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.set(true);
        serializer.serialize_str("later")
    }
}

#[derive(serde::Serialize)]
struct JsonSearchSerializationProbe<'a> {
    first: &'a str,
    tail: &'a SerializationReached,
}

fn rows() -> Vec<Vec<Value>> {
    vec![
        vec![Value::Int(1), Value::Text("Alice Smith".into())],
        vec![Value::Int(2), Value::Null],
        vec![Value::Int(3), Value::Text("bob alice".into())],
        vec![Value::Int(4), Value::Text("carol".into())],
    ]
}

fn query(needle: &str) -> FindQuery {
    FindQuery::new(needle, TextMatchOptions::default(), None)
}

fn find(needle: &str, limit: usize) -> Vec<(u32, usize)> {
    find_with(query(needle), limit)
}

fn find_with(query: FindQuery, limit: usize) -> Vec<(u32, usize)> {
    let rows = rows();
    matching_rows(
        rows.len() as u32,
        |p, visit| {
            if let Some(cells) = rows.get(p as usize) {
                for (column, value) in cells.iter().enumerate() {
                    if visit(column, value) {
                        break;
                    }
                }
            }
        },
        &query,
        limit,
    )
    .unwrap()
    .into_iter()
    .map(|hit| (hit.position, hit.column))
    .collect()
}

fn first_matching_cell(cells: &[Option<Value>], needle: &CaseInsensitivePattern) -> Option<(usize, String)> {
    cells
        .iter()
        .enumerate()
        .filter_map(|(column, value)| value.as_ref().map(|value| (column, value)))
        .find_map(|(column, value)| matches_value(value, needle).then(|| (column, value_to_display_text(value))))
}

#[test]
fn a_match_is_case_insensitive_and_reports_the_first_matching_column() {
    assert_eq!(find("ALICE", 10), vec![(0, 1), (2, 1)]);
    assert_eq!(find("3", 10), vec![(2, 0)]);
}

#[test]
fn match_options_and_column_scope_narrow_loaded_row_search() {
    let cased = FindQuery::new(
        "alice",
        TextMatchOptions {
            match_case: true,
            ..TextMatchOptions::default()
        },
        None,
    );
    assert_eq!(find_with(cased, 10), vec![(2, 1)]);
    let whole = FindQuery::new(
        "alice",
        TextMatchOptions {
            whole_words: true,
            ..TextMatchOptions::default()
        },
        None,
    );
    assert_eq!(find_with(whole, 10), vec![(0, 1), (2, 1)]);
    let scoped = FindQuery::new("3", TextMatchOptions::default(), Some(1));
    assert!(find_with(scoped, 10).is_empty());
    let regex = FindQuery::new(
        r"bob\s+alice",
        TextMatchOptions {
            regex: true,
            ..TextMatchOptions::default()
        },
        None,
    );
    assert_eq!(find_with(regex, 10), vec![(2, 1)]);
}

#[test]
fn case_insensitive_search_keeps_unicode_lowercase_expansions() {
    assert!(CaseInsensitivePattern::new("i\u{307}").matches("before İ after"));
}

#[test]
fn typed_json_search_matches_its_compact_serialized_representation() {
    let json = serde_json::json!({"Name": "Zoë", "count": 42});
    let value = Value::Json(Box::new(json.clone()));
    let serialized = json.to_string();
    for query in ["name", "ZOË", "42", "missing"] {
        let pattern = CaseInsensitivePattern::new(query);
        assert_eq!(matches_value(&value, &pattern), pattern.matches(&serialized));
    }
}

#[test]
fn typed_json_search_matches_unicode_split_across_writer_chunks() {
    let pattern = CaseInsensitivePattern::new("😀i\u{307}");
    let serialized = serde_json::json!({"emoji": "😀İ"}).to_string();
    let mut writer = JsonSearchWriter::new(&pattern);
    for byte in serialized.bytes() {
        if writer.write_all(&[byte]).is_err() {
            break;
        }
    }
    assert!(writer.matches());
}

#[test]
fn typed_json_search_stops_serialization_after_a_match() {
    let pattern = CaseInsensitivePattern::new("needle");
    let mut writer = JsonSearchWriter::new(&pattern);
    let tail = SerializationReached(std::cell::Cell::new(false));
    let probe = JsonSearchSerializationProbe {
        first: "needle",
        tail: &tail,
    };
    let error = serde_json::to_writer(&mut writer, &probe).unwrap_err();
    assert!(error.is_io());
    assert!(writer.matches());
    assert!(!tail.0.get());
}

#[test]
fn an_empty_query_matches_nothing() {
    assert!(find("", 10).is_empty());
    assert!(find("   ", 10).is_empty());
}

#[test]
fn a_missing_projected_cell_is_not_a_search_match() {
    let found = matching_rows(
        1,
        |_, visit| {
            visit(0, &Value::Int(1));
        },
        &query("not fetched"),
        10,
    )
    .unwrap();
    assert!(found.is_empty());
}

#[test]
fn null_binary_and_undecodable_values_keep_their_search_labels() {
    let cells = vec![
        Some(Value::Null),
        Some(Value::Bytes(vec![0xFF, 0xFE])),
        Some(Value::Undecodable("NUMERIC".into())),
    ];
    assert_eq!(
        first_matching_cell(&cells, &CaseInsensitivePattern::new("null"))
            .unwrap()
            .0,
        0
    );
    assert_eq!(
        first_matching_cell(&cells, &CaseInsensitivePattern::new("2 bytes"))
            .unwrap()
            .0,
        1
    );
    assert_eq!(
        first_matching_cell(&cells, &CaseInsensitivePattern::new("numeric"))
            .unwrap()
            .0,
        2
    );
}

#[test]
fn a_long_cell_remains_searchable_beyond_its_visible_preview() {
    let text = format!("{}needle", "😀".repeat(10_001));
    assert!(!value_to_display_text(&Value::Text(text.clone())).contains("needle"));
    let result = std::sync::Arc::new(bookie_core::QueryResult {
        columns: Vec::new(),
        rows: vec![vec![Value::Int(1), Value::Text(text)]].into(),
        truncated: false,
    });
    let store = crate::ui::row_store::RowStore::from_shared_with_previews(result, vec![0]);

    let found = matching_rows(
        1,
        |position, visit| {
            store.visit_search_cells(position, visit);
        },
        &query("needle"),
        10,
    )
    .unwrap();
    assert_eq!(
        found.iter().map(|hit| (hit.position, hit.column)).collect::<Vec<_>>(),
        vec![(0, 1)]
    );
}

#[test]
fn the_scan_stops_at_the_limit() {
    assert_eq!(find("a", 2), vec![(0, 1), (2, 1)]);
    assert_eq!(find("a", 1), vec![(0, 1)]);
}

#[test]
fn a_long_cell_is_shown_as_one_short_line() {
    let shown = preview(&format!("first\nsecond {}", "x".repeat(200)));
    assert!(!shown.contains('\n'));
    assert_eq!(shown.chars().count(), PREVIEW_CHARS + 1);
    assert!(shown.ends_with('…'));
}

#[test]
fn replace_edits_keyed_text_and_skips_read_only_or_undecodable_cells() {
    let rows = [
        vec![Value::Int(1), Value::Text("alpha".into()), Value::Text("keep".into())],
        vec![
            Value::Int(2),
            Value::Undecodable("numeric".into()),
            Value::Text("alpha".into()),
        ],
    ];
    let editable = [false, true, false];
    let report = plan_replacements(
        rows.len() as u32,
        |position, visit| {
            for (column, value) in rows[position as usize].iter().enumerate() {
                visit(column, value);
            }
        },
        |position| Some(vec![rows[position as usize][0].clone()]),
        &query("alpha"),
        "beta",
        &editable,
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(report.edits.len(), 1);
    assert_eq!(report.edits[0].column, 1);
    assert_eq!(report.edits[0].new_value, "beta");
    assert_eq!(report.replacements, 1);
    assert!(report.skipped >= 2);
}

#[test]
fn replace_stops_when_cancelled() {
    let rows = vec![vec![Value::Text("alpha".into())]; 8];
    let cancel = CancellationToken::new();
    cancel.cancel();
    let report = plan_replacements(
        rows.len() as u32,
        |position, visit| visit(0, &rows[position as usize][0]),
        |position| Some(vec![Value::Int(position as i64)]),
        &query("alpha"),
        "beta",
        &[true],
        &cancel,
    );
    assert!(matches!(report, Err(super::replace::ReplaceError::Cancelled)));
}

#[test]
fn cancellation_during_replacement_does_not_return_a_partial_plan() {
    let cancel = CancellationToken::new();
    let result = plan_replacements(
        2,
        |position, visit| {
            if position == 1 {
                cancel.cancel();
            }
            visit(0, &Value::Text("alpha".into()));
        },
        |position| Some(vec![Value::Int(i64::from(position))]),
        &query("alpha"),
        "beta",
        &[true],
        &cancel,
    );
    assert!(result.is_err(), "cancelled planning must not stage a partial result");
}

#[test]
fn large_shared_value_search_has_bounded_temporary_memory() {
    const KIND_ENV: &str = "BOOKIE_LARGE_ROW_SEARCH_KIND";
    if let Ok(kind) = std::env::var(KIND_ENV) {
        let mut text = String::with_capacity(64 * 1024 * 1024);
        text.extend(std::iter::repeat_n('x', 64 * 1024 * 1024 - 6));
        text.push_str("needle");
        let value = match kind.as_str() {
            "text" => Value::Text(text),
            "json" => Value::Json(Box::new(serde_json::Value::String(text))),
            _ => panic!("unknown row search fixture kind"),
        };
        let result = std::sync::Arc::new(bookie_core::QueryResult {
            columns: Vec::new(),
            rows: vec![vec![value]].into(),
            truncated: false,
        });
        let store = crate::ui::row_store::RowStore::from_shared(result);
        let before = peak_memory_kib();
        let found = matching_rows(
            1,
            |position, visit| {
                store.visit_search_cells(position, visit);
            },
            &query("needle"),
            1,
        )
        .unwrap();
        let extra = peak_memory_kib().saturating_sub(before);
        eprintln!("row-search-memory kind={kind} extra_peak_kib={extra}");
        assert_eq!(found.len(), 1);
        assert!(
            extra < 16 * 1024,
            "large-value search allocated {extra} KiB beyond its source"
        );
        return;
    }

    for kind in ["text", "json"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ui::grid::row_find::tests::large_shared_value_search_has_bounded_temporary_memory",
                "--nocapture",
            ])
            .env(KIND_ENV, kind)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{kind} row search exceeded its memory budget:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn peak_memory_kib() -> usize {
    std::fs::read_to_string("/proc/self/status")
        .expect("process memory status")
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:")?.split_whitespace().next()?.parse().ok())
        .expect("peak resident memory")
}
