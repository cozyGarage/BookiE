use std::io::{BufRead, BufReader, Read};

use crate::sql_text::{EDITOR_MAX_SQL_BYTES, EDITOR_MAX_SQL_LINE_BYTES, SqlTextError, SqlTextLimits};
use bookie_core::text_file::{FileVersion, TextFile, TextFileError, read_text_file, save_text_file};

pub const MAX_SQL_FILE_BYTES: u64 = EDITOR_MAX_SQL_BYTES;

pub fn first_overlong_sql_line(reader: &mut impl BufRead) -> std::io::Result<Option<usize>> {
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

pub fn validate_sql_file_lines(path: &std::path::Path) -> Result<(), String> {
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

pub fn read_sql_file(path: &std::path::Path) -> Result<TextFile, String> {
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

pub fn validate_sql_text(text: &str) -> Result<(), String> {
    SqlTextLimits::EDITOR
        .validate_document(text)
        .map_err(sql_text_error_message)
}

pub fn validate_sql_line_lengths(text: &str) -> Result<(), String> {
    validate_sql_text(text)
}

pub fn sql_text_error_message(error: SqlTextError) -> String {
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

pub fn validate_dropped_sql(current: &str, cursor: usize, text: &str) -> Result<(), String> {
    SqlTextLimits::EDITOR
        .validate_drop(current, cursor, text)
        .map_err(|error| match error {
            SqlTextError::InsertionTooLarge => {
                crate::tr!("Adding this SQL file would exceed the editor's 8 MiB limit. Open it in a separate tab.")
            }
            other => sql_text_error_message(other),
        })
}

pub fn save_sql_file(
    path: &std::path::Path,
    text: &str,
    expected: Option<&FileVersion>,
) -> Result<TextFile, TextFileError> {
    if text.len() as u64 > MAX_SQL_FILE_BYTES {
        return Err(TextFileError::TooLarge(MAX_SQL_FILE_BYTES));
    }
    save_text_file(path, text, expected)
}

pub fn file_error_message(error: &TextFileError) -> String {
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

#[cfg(test)]
#[path = "sql_file_tests.rs"]
mod tests;
