use super::*;
use std::io::{BufReader, Cursor, Write};

#[test]
fn an_overlong_line_is_detected_before_reading_the_rest_of_the_file() {
    let mut cursor = Cursor::new(vec![b'x'; EDITOR_MAX_SQL_LINE_BYTES * 32]);
    {
        let mut reader = BufReader::with_capacity(8192, &mut cursor);
        assert_eq!(first_overlong_sql_line(&mut reader).unwrap(), Some(1));
    }
    assert!(cursor.position() <= (EDITOR_MAX_SQL_LINE_BYTES + 8192) as u64);
}

#[test]
fn the_line_limit_excludes_a_crlf_terminator() {
    let mut text = vec![b'x'; EDITOR_MAX_SQL_LINE_BYTES];
    text.extend_from_slice(b"\r\n");
    let mut reader = BufReader::new(Cursor::new(text));
    assert_eq!(first_overlong_sql_line(&mut reader).unwrap(), None);
}

fn written(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("script.sql");
    let mut file = std::fs::File::create(&path).expect("create");
    file.write_all(bytes).expect("write");
    (dir, path)
}

#[test]
fn an_overlong_line_is_refused_without_changing_the_file() {
    let text = format!("--{}\n", "x".repeat(256 * 1024 - 1));
    let (_dir, path) = written(text.as_bytes());
    assert!(read_sql_file(&path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn a_line_at_the_boundary_keeps_exact_utf8_and_crlf_content() {
    for text in [
        format!("{}\r\nSELECT 1;\r\n", "x".repeat(256 * 1024)),
        format!("{}\nSELECT 1;\n", "é".repeat(128 * 1024)),
    ] {
        let (_dir, path) = written(text.as_bytes());
        assert_eq!(read_sql_file(&path).unwrap().text, text);
    }
}

#[test]
fn short_lines_still_fill_the_existing_eight_mib_file_budget() {
    let text = format!("--{}\n", "x".repeat(125)).repeat(65_536);
    assert_eq!(text.len(), 8 * 1024 * 1024);
    let (_dir, path) = written(text.as_bytes());
    assert_eq!(read_sql_file(&path).unwrap().text, text);
}

#[test]
fn an_existing_large_line_can_still_be_saved_without_truncation() {
    let text = "x".repeat(256 * 1024 + 1);
    let (_dir, path) = written(b"SELECT 1;");
    save_sql_file(&path, &text, None).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), text);
}

#[test]
fn individually_safe_dropped_lines_cannot_form_an_overlong_editor_line() {
    let current = "x".repeat(200 * 1024);
    let dropped = "é".repeat(100 * 1024);
    assert!(validate_sql_line_lengths(&current).is_ok());
    assert!(validate_sql_line_lengths(&dropped).is_ok());
    assert!(validate_dropped_sql(&current, current.len(), &dropped).is_err());
}

#[test]
fn dropped_sql_preserves_unicode_cursor_and_exact_line_boundary() {
    let current = format!("é{}\nSELECT 1;", "x".repeat(256 * 1024 - 4));
    assert!(validate_dropped_sql(&current, "é".len(), "😀").is_err());
    assert!(validate_dropped_sql(&current, "é".len(), "xy").is_ok());
    assert!(validate_dropped_sql(&current, 1, "x").is_err());
}

#[test]
fn dropping_files_cannot_accumulate_past_the_total_editor_budget() {
    let current = "SELECT 1;\n".repeat(800_000);
    let dropped = "SELECT 2;\n".repeat(100_000);
    assert!(validate_dropped_sql(&current, 0, &dropped).is_err());
    assert!(validate_dropped_sql("  \n", 0, "SELECT 1;").is_ok());
}

#[test]
fn a_chosen_sql_file_is_read_as_text() {
    let (_dir, path) = written(b"SELECT 1;\n");

    assert_eq!(read_sql_file(&path).unwrap().text, "SELECT 1;\n");
}

#[test]
fn a_file_over_the_size_limit_is_refused_rather_than_truncated() {
    let oversized = vec![b'-'; (MAX_SQL_FILE_BYTES + 1) as usize];
    let (_dir, path) = written(&oversized);

    let error = read_sql_file(&path).unwrap_err();

    assert!(error.contains("too large"), "{error}");
}

#[test]
fn a_file_that_is_not_text_is_refused() {
    let (_dir, path) = written(&[0xff, 0xfe, 0x00]);

    assert!(read_sql_file(&path).is_err());
}

#[test]
fn a_draft_over_the_size_limit_is_not_saved() {
    let (_dir, path) = written(b"SELECT 1;\n");
    let oversized = "-".repeat((MAX_SQL_FILE_BYTES + 1) as usize);

    let error = save_sql_file(&path, &oversized, None).unwrap_err();

    assert!(matches!(error, TextFileError::TooLarge(_)));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "SELECT 1;\n");
}
