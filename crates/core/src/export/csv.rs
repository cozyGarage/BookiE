use std::io::{self, Write};

use serde::{Deserialize, Serialize};

use super::error::ExportError;
use super::file::ResultWriter;
use super::value_to_text;
use crate::query::{ColumnInfo, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CsvDelimiter {
    Comma,
    Semicolon,
    Tab,
    Pipe,
}

impl CsvDelimiter {
    pub const ALL: [Self; 4] = [Self::Comma, Self::Semicolon, Self::Tab, Self::Pipe];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Comma => ",",
            Self::Semicolon => ";",
            Self::Tab => "\t",
            Self::Pipe => "|",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CsvQuote {
    Always,
    IfNeeded,
    Never,
}

impl CsvQuote {
    pub const ALL: [Self; 3] = [Self::Always, Self::IfNeeded, Self::Never];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CsvLineBreak {
    Lf,
    CrLf,
    Cr,
}

impl CsvLineBreak {
    pub const ALL: [Self; 3] = [Self::Lf, Self::CrLf, Self::Cr];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CsvDecimal {
    Period,
    Comma,
}

impl CsvDecimal {
    pub const ALL: [Self; 2] = [Self::Period, Self::Comma];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CsvOptions {
    pub null_to_empty: bool,
    pub null_marker: Option<String>,
    pub line_break_to_space: bool,
    pub header_row: bool,
    pub sanitize_formulas: bool,
    pub spare_plain_numbers: bool,
    /// Encode non-NULL cells with SQLite's storage-class tags when result
    /// metadata is unavailable. Enable only for SQLite result exports.
    pub preserve_sqlite_result_types: bool,
    pub delimiter: CsvDelimiter,
    pub quote: CsvQuote,
    pub line_break: CsvLineBreak,
    pub decimal: CsvDecimal,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            null_to_empty: true,
            null_marker: None,
            line_break_to_space: false,
            header_row: true,
            sanitize_formulas: true,
            spare_plain_numbers: false,
            preserve_sqlite_result_types: false,
            delimiter: CsvDelimiter::Comma,
            quote: CsvQuote::IfNeeded,
            line_break: CsvLineBreak::Lf,
            decimal: CsvDecimal::Period,
        }
    }
}

pub fn render_csv(columns: &[ColumnInfo], rows: &[Vec<Value>], options: &CsvOptions) -> String {
    let mut output = String::new();
    if options.header_row {
        output.push_str(&csv_header_line(columns, options));
        output.push_str(options.line_break.as_str());
    }
    for row in rows {
        output.push_str(&csv_row_line_for_columns(row, columns, options));
        output.push_str(options.line_break.as_str());
    }
    output
}

pub fn unique_csv_null_marker(rows: &[Vec<Value>]) -> String {
    let mut marker = "\\N".to_string();
    while rows
        .iter()
        .flatten()
        .any(|value| value_to_text(value).as_deref() == Some(marker.as_str()))
    {
        marker.push('N');
    }
    marker
}

pub(crate) fn csv_header_line(columns: &[ColumnInfo], options: &CsvOptions) -> String {
    columns
        .iter()
        .map(|column| escape_csv_field(&column.name, options, false))
        .collect::<Vec<_>>()
        .join(options.delimiter.as_str())
}

pub(crate) fn csv_row_line(row: &[Value], options: &CsvOptions) -> String {
    csv_row_line_with_any_columns(row, &[], options)
}

fn csv_row_line_for_columns(row: &[Value], columns: &[ColumnInfo], options: &CsvOptions) -> String {
    let any_columns = columns
        .iter()
        .map(|column| {
            crate::sqlite_any_csv::is_any_type(&column.data_type)
                || (options.preserve_sqlite_result_types && crate::sqlite_any_csv::is_unknown_type(&column.data_type))
        })
        .collect::<Vec<_>>();
    csv_row_line_with_any_columns(row, &any_columns, options)
}

fn csv_row_line_with_any_columns(row: &[Value], any_columns: &[bool], options: &CsvOptions) -> String {
    row.iter()
        .enumerate()
        .map(|(index, value)| {
            if any_columns.get(index) == Some(&true)
                && let Some(encoded) = crate::sqlite_any_csv::encode(value)
            {
                return format_csv_cell(&Value::Text(encoded), options);
            }
            format_csv_cell(value, options)
        })
        .collect::<Vec<_>>()
        .join(options.delimiter.as_str())
}

pub fn render_tsv(columns: &[ColumnInfo], rows: &[Vec<Value>], with_headers: bool) -> String {
    let options = CsvOptions {
        header_row: with_headers,
        null_to_empty: false,
        null_marker: Some(unique_csv_null_marker(rows)),
        delimiter: CsvDelimiter::Tab,
        ..CsvOptions::default()
    };
    let mut lines = Vec::with_capacity(rows.len() + usize::from(with_headers));
    if with_headers {
        lines.push(csv_header_line(columns, &options));
    }
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .map(|value| match value_to_text(value) {
                Some(text) if is_null_marker_shaped(&text) => quote_field(&text),
                _ => format_csv_cell(value, &options),
            })
            .collect();
        lines.push(cells.join("\t"));
    }
    lines.join("\n")
}

fn is_null_marker_shaped(text: &str) -> bool {
    text.strip_prefix("\\N")
        .is_some_and(|rest| rest.bytes().all(|byte| byte == b'N'))
}

fn rfc4180_options(header_row: bool) -> CsvOptions {
    CsvOptions {
        header_row,
        sanitize_formulas: false,
        ..CsvOptions::default()
    }
}

/// Escape and write a CSV header line for `columns`.
pub fn write_csv_header(writer: &mut impl Write, columns: &[ColumnInfo]) -> io::Result<()> {
    let options = rfc4180_options(true);
    writeln!(writer, "{}", csv_header_line(columns, &options))
}

pub fn write_csv_header_with_options(
    writer: &mut impl Write,
    columns: &[ColumnInfo],
    options: &CsvOptions,
) -> io::Result<()> {
    writeln!(writer, "{}", csv_header_line(columns, options))
}

/// Escape and write one CSV data row.
pub fn write_csv_row(writer: &mut impl Write, row: &[Value]) -> io::Result<()> {
    let options = rfc4180_options(false);
    writeln!(writer, "{}", csv_row_line(row, &options))
}

pub fn write_csv_row_with_options(writer: &mut impl Write, row: &[Value], options: &CsvOptions) -> io::Result<()> {
    writeln!(writer, "{}", csv_row_line(row, options))
}

pub fn write_csv_row_with_columns_and_options(
    writer: &mut impl Write,
    columns: &[ColumnInfo],
    row: &[Value],
    options: &CsvOptions,
) -> io::Result<()> {
    writeln!(writer, "{}", csv_row_line_for_columns(row, columns, options))
}

fn is_plain_number(value: &str) -> bool {
    let unsigned = value.strip_prefix(['+', '-']).unwrap_or(value);
    (!unsigned.is_empty() && unsigned.chars().all(|character| character.is_ascii_digit())) || is_plain_decimal(value)
}

fn is_plain_decimal(value: &str) -> bool {
    let unsigned = match value.strip_prefix(['+', '-']) {
        Some(value) => value,
        None => value,
    };
    let Some((integer, fraction)) = unsigned.split_once('.') else {
        return false;
    };
    !integer.is_empty()
        && !fraction.is_empty()
        && integer.chars().all(|character| character.is_ascii_digit())
        && fraction.chars().all(|character| character.is_ascii_digit())
}

pub(crate) fn quote_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn escape_csv_field(value: &str, options: &CsvOptions, had_line_breaks: bool) -> String {
    let mut value = value.to_string();
    let neutralized = options.sanitize_formulas
        && value.starts_with(['=', '+', '-', '@', '\t', '\r'])
        && !(options.spare_plain_numbers && is_plain_number(&value));
    if neutralized {
        value.insert(0, '\'');
    }
    match options.quote {
        CsvQuote::Always => quote_field(&value),
        CsvQuote::Never => value,
        CsvQuote::IfNeeded => {
            if value.contains(options.delimiter.as_str())
                || value.contains(['"', '\n', '\r', '\t'])
                || had_line_breaks
                || neutralized
            {
                quote_field(&value)
            } else {
                value
            }
        }
    }
}

fn format_csv_cell(value: &Value, options: &CsvOptions) -> String {
    let mut options = options.clone();
    options.sanitize_formulas &= matches!(value, Value::Text(_));
    let options = &options;
    let Some(mut text) = value_to_text(value) else {
        if let Some(marker) = &options.null_marker {
            return escape_csv_field(marker, options, false);
        }
        if options.null_to_empty {
            return String::new();
        }
        return escape_csv_field("NULL", options, false);
    };
    if text.is_empty() && options.quote != CsvQuote::Never {
        return quote_field(&text);
    }
    let had_line_breaks = text.contains(['\n', '\r']);
    if options.line_break_to_space {
        text = text.replace("\r\n", " ").replace(['\r', '\n'], " ");
    }
    if options.decimal == CsvDecimal::Comma && is_plain_decimal(&text) {
        text = text.replace('.', ",");
    }
    escape_csv_field(&text, options, had_line_breaks)
}

pub(crate) struct CsvWriter {
    header: CsvOptions,
    rows: CsvOptions,
    any_columns: Vec<bool>,
}

impl CsvWriter {
    pub(crate) fn new(options: &CsvOptions) -> Self {
        Self {
            header: options.clone(),
            rows: CsvOptions {
                header_row: false,
                ..options.clone()
            },
            any_columns: Vec::new(),
        }
    }
}

impl ResultWriter for CsvWriter {
    fn begin(&mut self, output: &mut dyn Write, columns: &[ColumnInfo]) -> Result<(), ExportError> {
        self.any_columns = columns
            .iter()
            .map(|column| {
                crate::sqlite_any_csv::is_any_type(&column.data_type)
                    || (self.header.preserve_sqlite_result_types
                        && crate::sqlite_any_csv::is_unknown_type(&column.data_type))
            })
            .collect();
        if !self.header.header_row {
            return Ok(());
        }
        output.write_all(csv_header_line(columns, &self.header).as_bytes())?;
        output.write_all(self.header.line_break.as_str().as_bytes())?;
        Ok(())
    }

    fn write_row(&mut self, output: &mut dyn Write, _index: usize, row: &[Value]) -> Result<(), ExportError> {
        output.write_all(csv_row_line_with_any_columns(row, &self.any_columns, &self.rows).as_bytes())?;
        output.write_all(self.rows.line_break.as_str().as_bytes())?;
        Ok(())
    }

    fn finish(&mut self, _output: &mut dyn Write) -> Result<(), ExportError> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "csv_tests.rs"]
mod tests;
