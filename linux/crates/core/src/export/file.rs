use std::io::{self, Write};
use std::path::Path;

use super::csv::{CsvOptions, CsvWriter};
use super::html::HtmlWriter;
use super::json::JsonWriter;
use super::markdown::MarkdownWriter;
use super::write_atomically_checked;
use super::xml::XmlWriter;
use crate::QueryResult;
use crate::query::{ColumnInfo, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultFormat {
    Csv,
    Json,
    Markdown,
    Html,
    Xml,
}

pub(crate) trait ResultWriter {
    fn begin(&mut self, output: &mut dyn Write, columns: &[ColumnInfo]) -> io::Result<()>;

    fn write_row(&mut self, output: &mut dyn Write, index: usize, row: &[Value]) -> io::Result<()>;

    fn finish(&mut self, output: &mut dyn Write) -> io::Result<()>;
}

fn writer_for(format: ResultFormat, options: &CsvOptions) -> Box<dyn ResultWriter> {
    match format {
        ResultFormat::Csv => Box::new(CsvWriter::new(options)),
        ResultFormat::Json => Box::new(JsonWriter::new()),
        ResultFormat::Markdown => Box::new(MarkdownWriter::new()),
        ResultFormat::Html => Box::new(HtmlWriter),
        ResultFormat::Xml => Box::new(XmlWriter::new()),
    }
}

/// Export an already materialized result. Extra memory is bounded to one row.
/// Cancellation is cooperative; publishing a completed file is the commit point.
pub fn write_result_file(
    path: &Path,
    result: &QueryResult,
    format: ResultFormat,
    options: &CsvOptions,
    cancelled: impl Fn() -> bool,
    progress: impl Fn(usize),
) -> io::Result<()> {
    let check = || {
        if cancelled() {
            Err(io::Error::new(io::ErrorKind::Interrupted, "Export cancelled"))
        } else {
            Ok(())
        }
    };
    check()?;
    let mut writer = writer_for(format, options);
    write_atomically_checked(
        path,
        |output| {
            writer.begin(output, &result.columns)?;
            for (index, row) in result.rows.iter().enumerate() {
                check()?;
                writer.write_row(output, index, row)?;
                progress(index + 1);
            }
            writer.finish(output)
        },
        check,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::render_csv;
    use crate::export::test_support::column;

    fn result() -> QueryResult {
        QueryResult {
            columns: ["id", "id", "id_2"].map(column).to_vec(),
            rows: vec![vec![
                Value::Bytes(vec![0, 255]),
                Value::Text("=1+1".into()),
                Value::Int(-42),
            ]],
            truncated: false,
        }
    }

    fn exported(format: ResultFormat) -> String {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result");
        write_result_file(&path, &result(), format, &CsvOptions::default(), || false, |_| {}).unwrap();
        std::fs::read_to_string(&path).unwrap()
    }

    #[test]
    fn file_and_clipboard_agree_and_cancellation_preserves_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result");
        let data = result();
        write_result_file(
            &path,
            &data,
            ResultFormat::Json,
            &CsvOptions::default(),
            || false,
            |_| {},
        )
        .unwrap();
        let actual: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let expected: serde_json::Value =
            serde_json::from_str(&crate::export::render_json(&data.columns, &data.rows)).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual[0]["id"], "\\x00ff");
        let before = std::fs::read(&path).unwrap();
        let cancel = std::cell::Cell::new(false);
        let error = write_result_file(
            &path,
            &data,
            ResultFormat::Csv,
            &CsvOptions::default(),
            || cancel.get(),
            |_| cancel.set(true),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn safe_csv_preserves_numbers_and_raw_mode_preserves_text() {
        let data = result();
        let safe = render_csv(&data.columns, &data.rows, &CsvOptions::default());
        assert!(safe.contains("\"'=1+1\""));
        assert!(safe.contains(",-42\n"));
        let raw = render_csv(
            &data.columns,
            &data.rows,
            &CsvOptions {
                sanitize_formulas: false,
                ..Default::default()
            },
        );
        assert!(raw.contains(",=1+1,-42\n"));
    }

    #[test]
    fn a_streamed_markdown_file_matches_the_clipboard_renderer() {
        let data = result();
        let expected = format!("{}\n", crate::export::render_markdown(&data.columns, &data.rows));
        assert_eq!(exported(ResultFormat::Markdown), expected);
    }

    #[test]
    fn a_streamed_html_file_is_a_complete_document() {
        let html = exported(ResultFormat::Html);
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        assert!(html.trim_end().ends_with("</html>"), "{html}");
        assert!(html.contains("<td>=1+1</td>"), "{html}");
    }

    #[test]
    fn a_streamed_xml_file_declares_its_encoding_and_closes_its_root() {
        let xml = exported(ResultFormat::Xml);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"), "{xml}");
        assert!(xml.contains("<id>\\x00ff</id>"), "{xml}");
        assert!(xml.trim_end().ends_with("</rows>"), "{xml}");
    }

    #[test]
    fn every_format_stops_at_the_first_cancelled_row_without_touching_the_destination() {
        for format in [
            ResultFormat::Csv,
            ResultFormat::Json,
            ResultFormat::Markdown,
            ResultFormat::Html,
            ResultFormat::Xml,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("result");
            let error =
                write_result_file(&path, &result(), format, &CsvOptions::default(), || true, |_| {}).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Interrupted, "{format:?}");
            assert!(!path.exists(), "{format:?}");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "{format:?}");
        }
    }
}
