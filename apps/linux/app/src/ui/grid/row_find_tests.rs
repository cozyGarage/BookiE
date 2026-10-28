use super::*;

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

fn find(query: &str, limit: usize) -> Vec<(u32, usize)> {
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
        query,
        limit,
    )
    .into_iter()
    .map(|hit| (hit.position, hit.column))
    .collect()
}

#[test]
fn a_match_is_case_insensitive_and_reports_the_first_matching_column() {
    assert_eq!(find("ALICE", 10), vec![(0, 1), (2, 1)]);
    assert_eq!(find("3", 10), vec![(2, 0)]);
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
        "not fetched",
        10,
    );
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
    assert!(!crate::ui::grid::display::value_to_display_text(&Value::Text(text.clone())).contains("needle"));
    let result = std::sync::Arc::new(tablepro_core::QueryResult {
        columns: Vec::new(),
        rows: vec![vec![Value::Int(1), Value::Text(text)]],
        truncated: false,
    });
    let store = crate::ui::row_store::RowStore::from_shared_with_previews(result, vec![0]);

    let found = matching_rows(
        1,
        |position, visit| {
            store.visit_search_cells(position, visit);
        },
        "needle",
        10,
    );
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
        let result = std::sync::Arc::new(tablepro_core::QueryResult {
            columns: Vec::new(),
            rows: vec![vec![value]],
            truncated: false,
        });
        let store = crate::ui::row_store::RowStore::from_shared(result);
        let before = peak_memory_kib();
        let found = matching_rows(
            1,
            |position, visit| {
                store.visit_search_cells(position, visit);
            },
            "needle",
            1,
        );
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
