use super::edit::*;
use super::model::*;

fn shown(kind: &str, text: &str) -> Result<(String, String), &'static str> {
    parse_edit(kind, text).map(|cell| (cell.kind().to_string(), cell.display()))
}

fn ok(kind: &str, text: &str, expected: &str) {
    assert_eq!(
        shown(kind, text),
        Ok((kind.to_string(), expected.to_string())),
        "{kind} {text}"
    );
}

#[test]
fn an_edit_is_parsed_and_shown_in_the_same_canonical_form_as_the_tauri_prototype() {
    ok("integer", " 42 ", "42");
    ok("boolean", "Yes", "true");
    ok("decimal", "1.50", "1.50");
    ok("date", "2026-02-03", "2026-02-03");
    ok(
        "uuid",
        "67E55044-10B1-426F-9247-BB680E5FE0C8",
        "67e55044-10b1-426f-9247-bb680e5fe0c8",
    );
    ok("bytes", "0xdeadbeef", "\\xdeadbeef");
    ok("json", "{\"a\": 1}", "{\"a\":1}");
    ok("text", "  keep spaces ", "  keep spaces ");
}

#[test]
fn an_invalid_edit_names_the_problem_in_the_same_words_as_the_tauri_prototype() {
    for (kind, text, reason) in [
        ("integer", "4x2", "not a whole number"),
        ("boolean", "maybe", "not true or false"),
        ("decimal", "1,5", "not a number"),
        ("float", "NaN", "not a number"),
        ("date", "2026-13-40", "not a date"),
        ("time", "25:00", "not a time"),
        ("timestamptz", "yesterday", "not a timestamp"),
        ("uuid", "abc", "not a UUID"),
        ("json", "{oops", "not JSON"),
        ("bytes", "xyz", "not hexadecimal bytes"),
        ("bytes", "é", "not hexadecimal bytes"),
        ("undecodable", "x", "this cell cannot be edited"),
    ] {
        assert_eq!(shown(kind, text), Err(reason), "{kind} {text}");
    }
}

#[test]
fn only_the_label_state_and_typed_columns_of_a_decodable_cell_are_editable() {
    assert!(!is_editable(0, &Cell::Int(1)));
    assert!(is_editable(1, &Cell::Text("a".into())));
    assert!(is_editable(2, &Cell::Null));
    assert!(!is_editable(3, &Cell::Undecodable("numeric".into())));
    assert!(!is_editable(4, &Cell::Text("a".into())));
}

#[test]
fn null_and_raw_json_text_cells_keep_their_own_rules() {
    ok("json_text", "{\"raw\":  1}", "{\"raw\":  1}");
    assert_eq!(shown("null", "now text"), Ok(("text".into(), "now text".into())));
}
