use std::io::Write;
use std::path::Path;

use super::csv::{CsvOptions, CsvWriter};
use super::error::ExportError;
use super::html::HtmlWriter;
use super::json::JsonWriter;
use super::markdown::MarkdownWriter;
use super::sql::SqlWriter;
use super::write_atomically_checked;
use super::xlsx::XlsxWriter;
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
    Sql,
    Xlsx,
}

pub struct SqlTarget<'a> {
    pub driver_id: &'a str,
    pub schema: Option<&'a str>,
    pub table: &'a str,
}

pub struct ResultExport<'a> {
    pub format: ResultFormat,
    pub csv: &'a CsvOptions,
    pub sql: Option<SqlTarget<'a>>,
}

pub(crate) trait ResultWriter {
    fn begin(&mut self, output: &mut dyn Write, columns: &[ColumnInfo]) -> Result<(), ExportError>;

    fn write_row(&mut self, output: &mut dyn Write, index: usize, row: &[Value]) -> Result<(), ExportError>;

    fn finish(&mut self, output: &mut dyn Write) -> Result<(), ExportError>;
}

fn writer_for(export: &ResultExport<'_>, result: &QueryResult) -> Result<Box<dyn ResultWriter>, ExportError> {
    Ok(match export.format {
        ResultFormat::Csv => Box::new(CsvWriter::new(export.csv)),
        ResultFormat::Json => Box::new(JsonWriter::new()),
        ResultFormat::Markdown => Box::new(MarkdownWriter::new()),
        ResultFormat::Html => Box::new(HtmlWriter),
        ResultFormat::Xml => Box::new(XmlWriter::new()),
        ResultFormat::Sql => match &export.sql {
            Some(target) => Box::new(SqlWriter::new(target)?),
            None => return Err(ExportError::MissingSqlTarget),
        },
        ResultFormat::Xlsx => Box::new(XlsxWriter::new(result.rows.len())?),
    })
}

/// Export an already materialized result. Extra memory is bounded to one row.
/// Cancellation is cooperative; publishing a completed file is the commit point.
pub fn write_result_file(
    path: &Path,
    result: &QueryResult,
    export: &ResultExport<'_>,
    cancelled: impl Fn() -> bool,
    progress: impl Fn(usize),
) -> Result<(), ExportError> {
    let check = || {
        if cancelled() {
            Err(ExportError::Cancelled)
        } else {
            Ok(())
        }
    };
    check()?;
    let mut writer = writer_for(export, result)?;
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

    fn plain(format: ResultFormat, options: &CsvOptions) -> ResultExport<'_> {
        ResultExport {
            format,
            csv: options,
            sql: None,
        }
    }

    fn exported(format: ResultFormat) -> String {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result");
        let options = CsvOptions::default();
        write_result_file(&path, &result(), &plain(format, &options), || false, |_| {}).unwrap();
        std::fs::read_to_string(&path).unwrap()
    }

    #[test]
    fn file_and_clipboard_agree_and_cancellation_preserves_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result");
        let data = result();
        let options = CsvOptions::default();
        write_result_file(&path, &data, &plain(ResultFormat::Json, &options), || false, |_| {}).unwrap();
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
            &plain(ResultFormat::Csv, &options),
            || cancel.get(),
            |_| cancel.set(true),
        )
        .unwrap_err();
        assert!(error.is_cancelled(), "{error:?}");
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
    fn a_sql_export_without_a_table_is_refused_before_the_destination_is_touched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result");
        let options = CsvOptions::default();
        let error =
            write_result_file(&path, &result(), &plain(ResultFormat::Sql, &options), || false, |_| {}).unwrap_err();
        assert!(matches!(error, ExportError::MissingSqlTarget), "{error:?}");
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_sql_export_names_the_table_the_rows_came_from() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result");
        let options = CsvOptions::default();
        let export = ResultExport {
            format: ResultFormat::Sql,
            csv: &options,
            sql: Some(SqlTarget {
                driver_id: "postgres",
                schema: Some("public"),
                table: "people",
            }),
        };
        let mut data = result();
        data.rows[0][0] = Value::Int(1);
        write_result_file(&path, &data, &export, || false, |_| {}).unwrap();
        let sql = std::fs::read_to_string(&path).unwrap();
        assert!(
            sql.starts_with("INSERT INTO \"public\".\"people\" (\"id\", \"id\", \"id_2\")"),
            "{sql}"
        );
    }

    #[test]
    fn a_sql_export_with_binary_data_preserves_the_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result");
        std::fs::write(&path, "previous export").unwrap();
        let options = CsvOptions::default();
        let export = ResultExport {
            format: ResultFormat::Sql,
            csv: &options,
            sql: Some(SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "people",
            }),
        };
        let error = write_result_file(&path, &result(), &export, || false, |_| {}).unwrap_err();
        assert!(matches!(
            error,
            ExportError::Statement(crate::sql_dialect::BuildSqlError::UnrepresentableValue { .. })
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous export");
    }

    #[test]
    fn an_excel_export_over_the_row_limit_is_refused_before_the_destination_is_touched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result");
        let options = CsvOptions::default();
        let oversized = QueryResult {
            columns: ["id"].map(column).to_vec(),
            rows: vec![Vec::new(); crate::export::MAX_WORKBOOK_ROWS + 1],
            truncated: false,
        };
        let error = write_result_file(
            &path,
            &oversized,
            &plain(ResultFormat::Xlsx, &options),
            || false,
            |_| {},
        )
        .unwrap_err();
        assert!(
            matches!(error, ExportError::WorkbookTooLarge { limit, rows }
                if limit == crate::export::MAX_WORKBOOK_ROWS && rows == crate::export::MAX_WORKBOOK_ROWS + 1),
            "{error:?}"
        );
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn an_excel_export_publishes_a_complete_workbook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result");
        let options = CsvOptions::default();
        write_result_file(&path, &result(), &plain(ResultFormat::Xlsx, &options), || false, |_| {}).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..2], b"PK");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn every_format_stops_at_the_first_cancelled_row_without_touching_the_destination() {
        let options = CsvOptions::default();
        for format in [
            ResultFormat::Csv,
            ResultFormat::Json,
            ResultFormat::Markdown,
            ResultFormat::Html,
            ResultFormat::Xml,
            ResultFormat::Xlsx,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("result");
            let error = write_result_file(&path, &result(), &plain(format, &options), || true, |_| {}).unwrap_err();
            assert!(error.is_cancelled(), "{format:?}");
            assert!(!path.exists(), "{format:?}");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "{format:?}");
        }
    }
}
