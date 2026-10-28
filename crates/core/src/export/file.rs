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

pub(super) fn writer_for(export: &ResultExport<'_>, row_count: usize) -> Result<Box<dyn ResultWriter>, ExportError> {
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
        ResultFormat::Xlsx => Box::new(XlsxWriter::new(row_count)?),
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
    let mut writer = writer_for(export, result.rows.len())?;
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
#[path = "file_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stream_tests.rs"]
mod stream_tests;
