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
    pub line_break_to_space: bool,
    pub header_row: bool,
    pub sanitize_formulas: bool,
    pub delimiter: CsvDelimiter,
    pub quote: CsvQuote,
    pub line_break: CsvLineBreak,
    pub decimal: CsvDecimal,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            null_to_empty: true,
            line_break_to_space: false,
            header_row: true,
            sanitize_formulas: true,
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
        output.push_str(&csv_row_line(row, options));
        output.push_str(options.line_break.as_str());
    }
    output
}

pub(crate) fn csv_header_line(columns: &[ColumnInfo], options: &CsvOptions) -> String {
    columns
        .iter()
        .map(|column| escape_csv_field(&column.name, options, false))
        .collect::<Vec<_>>()
        .join(options.delimiter.as_str())
}

pub(crate) fn csv_row_line(row: &[Value], options: &CsvOptions) -> String {
    row.iter()
        .map(|value| format_csv_cell(value, options))
        .collect::<Vec<_>>()
        .join(options.delimiter.as_str())
}

pub fn render_tsv(columns: &[ColumnInfo], rows: &[Vec<Value>], with_headers: bool) -> String {
    let mut lines = Vec::with_capacity(rows.len() + usize::from(with_headers));
    if with_headers {
        lines.push(
            columns
                .iter()
                .map(|column| escape_tsv_field(&column.name))
                .collect::<Vec<_>>()
                .join("\t"),
        );
    }
    for row in rows {
        lines.push(
            row.iter()
                .map(|value| {
                    let text = match value_to_text(value) {
                        Some(text) => text,
                        None => "NULL".to_string(),
                    };
                    escape_tsv_field(&text)
                })
                .collect::<Vec<_>>()
                .join("\t"),
        );
    }
    lines.join("\n")
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

/// Escape and write one CSV data row.
pub fn write_csv_row(writer: &mut impl Write, row: &[Value]) -> io::Result<()> {
    let options = rfc4180_options(false);
    writeln!(writer, "{}", csv_row_line(row, &options))
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
    let neutralized = options.sanitize_formulas && value.starts_with(['=', '+', '-', '@', '\t', '\r']);
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
        return escape_csv_field(if options.null_to_empty { "" } else { "NULL" }, options, false);
    };
    let had_line_breaks = text.contains(['\n', '\r']);
    if options.line_break_to_space {
        text = text.replace("\r\n", " ").replace(['\r', '\n'], " ");
    }
    if options.decimal == CsvDecimal::Comma && is_plain_decimal(&text) {
        text = text.replace('.', ",");
    }
    escape_csv_field(&text, options, had_line_breaks)
}

fn escape_tsv_field(value: &str) -> String {
    if value.contains(['\t', '\n', '\r', '"']) {
        quote_field(value)
    } else {
        value.to_string()
    }
}

pub(crate) struct CsvWriter {
    header: CsvOptions,
    rows: CsvOptions,
}

impl CsvWriter {
    pub(crate) fn new(options: &CsvOptions) -> Self {
        Self {
            header: options.clone(),
            rows: CsvOptions {
                header_row: false,
                ..options.clone()
            },
        }
    }
}

impl ResultWriter for CsvWriter {
    fn begin(&mut self, output: &mut dyn Write, columns: &[ColumnInfo]) -> Result<(), ExportError> {
        if !self.header.header_row {
            return Ok(());
        }
        output.write_all(csv_header_line(columns, &self.header).as_bytes())?;
        output.write_all(self.header.line_break.as_str().as_bytes())?;
        Ok(())
    }

    fn write_row(&mut self, output: &mut dyn Write, _index: usize, row: &[Value]) -> Result<(), ExportError> {
        output.write_all(csv_row_line(row, &self.rows).as_bytes())?;
        output.write_all(self.rows.line_break.as_str().as_bytes())?;
        Ok(())
    }

    fn finish(&mut self, _output: &mut dyn Write) -> Result<(), ExportError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::test_support::column;

    #[test]
    fn csv_header_and_row() {
        let cols = vec![column("a,b")];
        let mut buf = Vec::new();
        write_csv_header(&mut buf, &cols).unwrap();
        write_csv_row(&mut buf, &[Value::Text("hello".into())]).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s, "\"a,b\"\nhello\n");
    }

    #[test]
    fn the_plain_csv_writers_keep_formula_text_and_render_null_as_an_empty_field() {
        let mut buf = Vec::new();
        write_csv_row(&mut buf, &[Value::Text("=SUM(A1)".into()), Value::Null]).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "=SUM(A1),\n");
    }

    #[test]
    fn csv_renderer_applies_export_options_without_losing_cell_content() {
        let columns = vec![column("formula"), column("amount")];
        let rows = vec![vec![
            Value::Text("=SUM(A1:A2)\r\nnext".into()),
            Value::Decimal("12.30".parse().unwrap()),
        ]];
        let options = CsvOptions {
            line_break_to_space: true,
            delimiter: CsvDelimiter::Semicolon,
            quote: CsvQuote::IfNeeded,
            line_break: CsvLineBreak::CrLf,
            decimal: CsvDecimal::Comma,
            ..CsvOptions::default()
        };

        assert_eq!(
            render_csv(&columns, &rows, &options),
            "formula;amount\r\n\"'=SUM(A1:A2) next\";12,30\r\n"
        );
    }

    #[test]
    fn tsv_renderer_quotes_structural_characters_and_represents_nulls() {
        let columns = vec![column("a\tb"), column("value")];
        let rows = vec![vec![Value::Text("one\ntwo\"three".into()), Value::Null]];

        assert_eq!(
            render_tsv(&columns, &rows, true),
            "\"a\tb\"\tvalue\n\"one\ntwo\"\"three\"\tNULL"
        );
    }
}
