use gpui::TestAppContext;

use crate::ui_tests::open;
use super::run_target;

const SCRIPT: &str = "SELECT 1;\nSELECT 2;\nSELECT 3;";

#[test]
fn the_statement_under_the_caret_is_the_one_to_run() {
    assert_eq!(run_target(SCRIPT, "postgres", 3, 3..3).as_deref(), Ok("SELECT 1"));
    assert_eq!(run_target(SCRIPT, "postgres", 14, 14..14).as_deref(), Ok("SELECT 2"));
    let end = SCRIPT.len();
    assert_eq!(run_target(SCRIPT, "postgres", end, end..end).as_deref(), Ok("SELECT 3"));
}

#[test]
fn a_selection_wins_over_the_caret_statement() {
    assert_eq!(run_target(SCRIPT, "postgres", 19, 10..19).as_deref(), Ok("SELECT 2;"));
    assert_eq!(run_target(SCRIPT, "postgres", 3, 7..9).as_deref(), Ok("1;"));
}

#[test]
fn a_blank_selection_or_script_has_nothing_to_run() {
    assert!(run_target("   \n  ", "postgres", 2, 2..2).is_err());
    assert!(run_target("", "postgres", 0, 0..0).is_err());
    assert!(
        run_target(SCRIPT, "postgres", 0, 9..10).is_err(),
        "a selected newline is blank"
    );
}

#[test]
fn an_unterminated_statement_is_refused_rather_than_guessed() {
    let error = run_target("SELECT 'oops", "postgres", 4, 4..4).unwrap_err();
    assert!(error.contains("complete statement"), "{error}");
}

#[test]
fn error_location_uses_server_character_offsets_only_for_unchanged_buffer() {
    let sql = "SELECT 'é';\nSELECT missing";
    let pos = sql.chars().position(|c| c == 'm').unwrap() + 1;
    assert_eq!(super::error_location(sql, sql, pos), Some((1, 7)));
    assert_eq!(super::error_location("changed", sql, pos), None);
    assert_eq!(super::error_location(sql, sql, 0), None);
    assert_eq!(super::error_location(sql, sql, 999), None);
    let emoji_sql = "SELECT '🎉', missing";
    let emoji_pos = emoji_sql.chars().position(|c| c == 'm').unwrap() + 1;
    assert_eq!(super::error_location(emoji_sql, emoji_sql, emoji_pos), Some((0, 12)));
}

#[test]
fn error_token_selects_an_identifier_on_character_boundaries() {
    let sql = "SELECT 🎉, naïve FROM t";
    let pos = sql.chars().position(|c| c == 'n').unwrap() + 1;
    assert_eq!(&sql[super::error_token(sql, pos).unwrap()], "naïve");
    assert_eq!(&sql[super::error_token(sql, pos + 2).unwrap()], "naïve");
    let sql = "SELECT :long_name, value::text";
    assert_eq!(&sql[super::error_token(sql, 8).unwrap()], ":long_name");
    assert_eq!(&sql[super::error_token(sql, 25).unwrap()], ":");
    assert_eq!(&sql[super::error_token(sql, 26).unwrap()], ":");
    assert!(super::error_token(sql, 0).is_none());
}

#[test]
fn source_range_tracks_selection_and_duplicate_caret_statements() {
    let sql = "SELECT 1;\nSELECT 1;";
    let range = super::run_source_range(sql, "postgres", 12, 12..12).unwrap();
    assert_eq!(&sql[range.clone()], "SELECT 1");
    assert_eq!(range.start, 10);
    let range = super::run_source_range("  SELECT x  ", "postgres", 0, 0..12).unwrap();
    assert_eq!(range, 2..10);
    assert!(super::run_source_range("é", "postgres", 0, 1..2).is_none());
}

#[test]
fn mapped_error_stays_inside_submitted_statement_and_unchanged_snapshot() {
    let editor = "SELECT 1;\nSELECT missing;\nSELECT later";
    let source = (editor.to_string(), 10..24);
    let (location, token) = super::mapped_error_location(editor, "SELECT missing", 8, Some(&source)).unwrap();
    assert_eq!(location, (1, 7));
    assert_eq!(&editor[token], "missing");
    assert!(super::mapped_error_location(editor, "SELECT missing", 15, Some(&source)).is_none());
    assert!(super::mapped_error_location("changed", "SELECT missing", 8, Some(&source)).is_none());
    let mismatched = (editor.to_string(), 0..8);
    assert!(super::mapped_error_location(editor, "SELECT missing", 8, Some(&mismatched)).is_none());
    let invalid = (editor.to_string(), 1..usize::MAX);
    assert!(super::mapped_error_location(editor, "SELECT missing", 8, Some(&invalid)).is_none());
}

#[test]
fn rewritten_parameter_error_maps_to_second_line_in_editor() {
    let editor = "SELECT :long_name,\n       missing FROM people";
    let parsed = bookie_core::extract_named_parameters(editor, "postgres");
    let server_position = parsed.sql.chars().position(|c| c == 'm').unwrap() + 1;
    let byte = parsed.source_byte_for_position(editor, server_position).unwrap();
    let original_position = editor[..byte].chars().count() + 1;
    let (location, token) = super::mapped_error_location(editor, editor, original_position, None).unwrap();
    assert_eq!(location, (1, 7));
    assert_eq!(&editor[token], "missing");
    assert!(super::mapped_error_location("changed", editor, original_position, None).is_none());
}

#[gpui::test]
fn error_mapping_selects_multiline_identifier_in_editor(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|window, cx| {
        let editor = view.read(cx).sql_input.clone();
        let sql = "SELECT 1;\nSELECT missing FROM t";
        let source = (sql.to_string(), 10..31);
        let (location, token) = crate::run_cursor_ui::mapped_error_location(sql, "SELECT missing FROM t", 8, Some(&source)).unwrap();
        editor.update(cx, |editor, cx| {
            editor.set_value(sql, window, cx);
            editor.set_cursor_position(gpui_kit::component::input::Position::new(location.0, location.1), window, cx);
            editor.set_selected_range(token, cx);
            assert_eq!(editor.selected_value().as_ref(), "missing");
        });
    });
}
