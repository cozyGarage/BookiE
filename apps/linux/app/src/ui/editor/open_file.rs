use relm4::gtk;
use relm4::gtk::prelude::*;
use std::io::{BufRead, BufReader, Read};
use tablepro_app_services::sql_text::{EDITOR_MAX_SQL_LINE_BYTES, SqlTextError, SqlTextLimits};
use tablepro_core::text_file::{FileVersion, TextFile, TextFileError, read_text_file, save_text_file};

use super::MAX_SQL_FILE_BYTES;

fn first_overlong_sql_line(reader: &mut impl BufRead) -> std::io::Result<Option<usize>> {
    let mut line = 1;
    let mut line_bytes = 0;
    let mut last_was_carriage_return = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok((line_bytes > EDITOR_MAX_SQL_LINE_BYTES).then_some(line));
        }
        let mut consumed = 0;
        let mut overlong = None;
        for byte in available {
            consumed += 1;
            if *byte == b'\n' {
                let content_bytes = line_bytes.saturating_sub(usize::from(last_was_carriage_return));
                if content_bytes > EDITOR_MAX_SQL_LINE_BYTES {
                    overlong = Some(line);
                    break;
                }
                line += 1;
                line_bytes = 0;
                last_was_carriage_return = false;
                continue;
            }
            line_bytes = line_bytes.saturating_add(1);
            last_was_carriage_return = *byte == b'\r';
            if line_bytes > EDITOR_MAX_SQL_LINE_BYTES.saturating_add(1)
                || (line_bytes > EDITOR_MAX_SQL_LINE_BYTES && !last_was_carriage_return)
            {
                overlong = Some(line);
                break;
            }
        }
        if overlong.is_some() {
            return Ok(overlong);
        }
        reader.consume(consumed);
    }
}

fn validate_sql_file_lines(path: &std::path::Path) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|error| file_error_message(&TextFileError::Io(error)))?;
    let mut reader = BufReader::new(file).take(MAX_SQL_FILE_BYTES.saturating_add(1));
    if let Some(line) =
        first_overlong_sql_line(&mut reader).map_err(|error| file_error_message(&TextFileError::Io(error)))?
    {
        return Err(sql_text_error_message(SqlTextError::LineTooLong { line }));
    }
    if reader.limit() == 0 {
        return Err(file_error_message(&TextFileError::TooLarge(MAX_SQL_FILE_BYTES)));
    }
    Ok(())
}

pub(crate) fn read_sql_file(path: &std::path::Path) -> Result<TextFile, String> {
    if std::fs::metadata(path)
        .map_err(|error| file_error_message(&TextFileError::Io(error)))?
        .len()
        <= MAX_SQL_FILE_BYTES
    {
        validate_sql_file_lines(path)?;
    }
    let file = read_text_file(path, MAX_SQL_FILE_BYTES).map_err(|error| file_error_message(&error))?;
    validate_sql_text(&file.text)?;
    Ok(file)
}

pub(crate) fn validate_sql_text(text: &str) -> Result<(), String> {
    SqlTextLimits::EDITOR
        .validate_document(text)
        .map_err(sql_text_error_message)
}

pub(super) fn validate_buffer_insert(
    buffer: &gtk::TextBuffer,
    location: &gtk::TextIter,
    text: &str,
    current_bytes: usize,
) -> Result<(), String> {
    let mut line_start = *location;
    line_start.set_line_offset(0);
    let mut line_end = *location;
    line_end.forward_to_line_end();
    let prefix_len = buffer.text(&line_start, location, true).len();
    let suffix_len = buffer.text(location, &line_end, true).len();
    let first_line = location.line().max(0) as usize + 1;
    SqlTextLimits::EDITOR
        .validate_insertion(current_bytes, prefix_len, suffix_len, first_line, text)
        .map_err(sql_text_error_message)
}

pub(super) fn validate_sql_line_lengths(text: &str) -> Result<(), String> {
    validate_sql_text(text)
}

fn sql_text_error_message(error: SqlTextError) -> String {
    match error {
        SqlTextError::DocumentTooLarge | SqlTextError::InsertionTooLarge => {
            crate::tr!("The SQL text exceeds the editor's 8 MiB limit.")
        }
        SqlTextError::LineTooLong { line } => crate::tr!(
            "The SQL text contains a line over {limit} KiB (line {line}). Split long lines before opening. The text has not been changed."
        )
        .replace("{limit}", &(EDITOR_MAX_SQL_LINE_BYTES / 1024).to_string())
        .replace("{line}", &line.to_string()),
        SqlTextError::InvalidCursor => crate::tr!("The insertion position changed. Drop the SQL file again."),
    }
}

pub(super) fn validate_dropped_sql(current: &str, cursor: usize, text: &str) -> Result<(), String> {
    SqlTextLimits::EDITOR
        .validate_drop(current, cursor, text)
        .map_err(|error| match error {
            SqlTextError::InsertionTooLarge => {
                crate::tr!("Adding this SQL file would exceed the editor's 8 MiB limit. Open it in a separate tab.")
            }
            other => sql_text_error_message(other),
        })
}

pub(crate) fn save_sql_file(
    path: &std::path::Path,
    text: &str,
    expected: Option<&FileVersion>,
) -> Result<TextFile, TextFileError> {
    if text.len() as u64 > MAX_SQL_FILE_BYTES {
        return Err(TextFileError::TooLarge(MAX_SQL_FILE_BYTES));
    }
    save_text_file(path, text, expected)
}

pub(crate) fn file_error_message(error: &TextFileError) -> String {
    match error {
        TextFileError::Changed => crate::tr!("The file changed on disk"),
        TextFileError::TooLarge(_) => crate::tr!("The SQL file is too large"),
        TextFileError::NotText => crate::tr!("The SQL file is not valid UTF-8"),
        TextFileError::Io(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            crate::tr!("You don't have permission to use that file")
        }
        TextFileError::Io(_) => crate::tr!("Couldn't read or write the SQL file"),
    }
}

fn sql_filters() -> gtk::gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&crate::tr!("SQL files")));
    filter.add_pattern("*.sql");
    filter.add_mime_type("application/sql");
    filter.add_mime_type("text/x-sql");
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}

fn sql_file_dialog(title: &str) -> gtk::FileDialog {
    let dialog = gtk::FileDialog::builder().title(title).modal(true).build();
    let filters = sql_filters();
    let default = filters.item(0).and_downcast::<gtk::FileFilter>();
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(default.as_ref());
    dialog
}

pub(crate) fn choose_sql_file<F>(parent: Option<gtk::Window>, done: F)
where
    F: FnOnce(Result<TextFile, String>) + 'static,
{
    let dialog = sql_file_dialog(&crate::tr!("Open SQL file"));
    dialog.open(parent.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
        let Ok(file) = result else {
            return;
        };
        let Some(path) = file.path() else {
            done(Err(crate::tr!("That file is not on this computer")));
            return;
        };
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(read_sql_file(&path));
        });
        relm4::spawn_local(async move {
            if let Ok(outcome) = rx.recv().await {
                done(outcome);
            }
        });
    });
}

pub(crate) fn choose_save_path<F>(parent: Option<gtk::Window>, suggested_name: &str, done: F)
where
    F: FnOnce(Option<Result<std::path::PathBuf, String>>) + 'static,
{
    let dialog = sql_file_dialog(&crate::tr!("Save SQL file"));
    dialog.set_initial_name(Some(suggested_name));
    dialog.save(parent.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
        let Ok(file) = result else {
            done(None);
            return;
        };
        match file.path() {
            Some(path) => done(Some(Ok(path))),
            None => done(Some(Err(crate::tr!("That file is not on this computer")))),
        }
    });
}

#[cfg(test)]
mod tests {
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
    fn dropped_sql_uses_the_same_long_line_boundary() {
        let text = "é".repeat(128 * 1024 + 1);
        let (_dir, path) = written(text.as_bytes());
        assert!(super::super::read_sql_text(&path, MAX_SQL_FILE_BYTES).is_err());
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
}
