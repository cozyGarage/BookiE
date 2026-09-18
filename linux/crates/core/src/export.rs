//! Streaming export helpers. CSV writes row-by-row without holding the
//! full result set.

use std::collections::HashSet;
#[cfg(test)]
use std::fs;
use std::io::{self, BufWriter, Write};

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::connection::Connection;
use crate::error::DriverError;
use crate::query::{ColumnInfo, Value};
use crate::sql_dialect::build_order_and_pagination;

mod file;
pub use file::{ResultFormat, write_result_file};

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

pub(crate) fn value_to_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(value) => Some(value.to_string()),
        Value::Int(value) => Some(value.to_string()),
        Value::Float(value) => Some(value.to_string()),
        Value::Text(value) => Some(value.clone()),
        Value::Bytes(value) => Some(format!("\\x{}", hex_encode(value))),
        Value::Date(value) => Some(value.to_string()),
        Value::Time(value) => Some(value.to_string()),
        Value::DateTime(value) => Some(value.to_string()),
        Value::TimestampTz(value) => Some(value.to_rfc3339()),
        Value::Decimal(value) => Some(value.to_string()),
        Value::Uuid(value) => Some(value.to_string()),
        Value::Json(value) => Some(value.to_string()),
        Value::Undecodable(type_name) => Some(undecodable_marker(type_name)),
    }
}

fn undecodable_marker(type_name: &str) -> String {
    format!("<undecodable {type_name}>")
}

pub fn render_csv(columns: &[ColumnInfo], rows: &[Vec<Value>], options: &CsvOptions) -> String {
    let delimiter = options.delimiter.as_str();
    let line_break = options.line_break.as_str();
    let mut output = String::new();
    if options.header_row {
        let header = columns
            .iter()
            .map(|column| escape_csv_field(&column.name, options, false))
            .collect::<Vec<_>>();
        output.push_str(&header.join(delimiter));
        output.push_str(line_break);
    }
    for row in rows {
        let fields = row
            .iter()
            .map(|value| format_csv_cell(value, options))
            .collect::<Vec<_>>();
        output.push_str(&fields.join(delimiter));
        output.push_str(line_break);
    }
    output
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

pub fn json_field_names(columns: &[ColumnInfo]) -> Vec<String> {
    let mut reserved = columns.iter().map(|column| column.name.clone()).collect::<HashSet<_>>();
    let mut emitted = HashSet::with_capacity(columns.len());
    let mut names = Vec::with_capacity(columns.len());
    for column in columns {
        let mut name = column.name.clone();
        if !emitted.insert(name.clone()) {
            let mut suffix = 2;
            loop {
                let candidate = format!("{}_{suffix}", column.name);
                if !reserved.contains(&candidate) && emitted.insert(candidate.clone()) {
                    name = candidate;
                    break;
                }
                suffix += 1;
            }
            reserved.insert(name.clone());
        }
        names.push(name);
    }
    names
}

pub fn row_to_json(columns: &[ColumnInfo], row: &[Value]) -> serde_json::Value {
    row_to_json_object(&json_field_names(columns), row)
}

pub fn render_json(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
    let names = json_field_names(columns);
    let values = rows
        .iter()
        .map(|row| row_to_json_object(&names, row))
        .collect::<Vec<_>>();
    match serde_json::to_string_pretty(&values) {
        Ok(output) => output,
        Err(_) => "[]".to_string(),
    }
}

pub fn render_markdown(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
    let mut lines = Vec::with_capacity(rows.len() + 2);
    lines.push(format!(
        "| {} |",
        columns
            .iter()
            .map(|column| markdown_cell(&column.name))
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    lines.push(format!("| {} |", vec!["---"; columns.len()].join(" | ")));
    for row in rows {
        lines.push(format!(
            "| {} |",
            row.iter().map(markdown_value_cell).collect::<Vec<_>>().join(" | ")
        ));
    }
    lines.join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InClause {
    pub sql: String,
    pub skipped: usize,
}

pub fn render_in_clause(driver_id: &str, rows: &[Vec<Value>], column_index: usize) -> InClause {
    let values = rows.iter().filter_map(|row| row.get(column_index)).collect::<Vec<_>>();
    let literals = values
        .iter()
        .filter_map(|value| in_clause_literal(driver_id, value))
        .collect::<Vec<_>>();
    InClause {
        skipped: values.len() - literals.len(),
        sql: if literals.is_empty() {
            String::new()
        } else {
            format!("({})", literals.join(", "))
        },
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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

fn quote_field(value: &str) -> String {
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

fn row_to_json_object(names: &[String], row: &[Value]) -> serde_json::Value {
    let mut object = serde_json::Map::with_capacity(names.len());
    for (index, name) in names.iter().enumerate() {
        let value = match row.get(index) {
            Some(value) => value_to_json(value),
            None => serde_json::Value::Null,
        };
        object.insert(name.clone(), value);
    }
    serde_json::Value::Object(object)
}

fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(value) => serde_json::Value::Bool(*value),
        Value::Int(value) => serde_json::Value::Number((*value).into()),
        Value::Float(value) => {
            serde_json::Number::from_f64(*value).map_or(serde_json::Value::Null, serde_json::Value::Number)
        }
        Value::Decimal(value) => match value.to_string().parse() {
            Ok(value) => serde_json::Value::Number(value),
            Err(_) => serde_json::Value::String(value.to_string()),
        },
        Value::Json(value) => value.clone(),
        _ => value_to_text(value).map_or(serde_json::Value::Null, serde_json::Value::String),
    }
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\r', '\n'], "<br>")
}

fn markdown_value_cell(value: &Value) -> String {
    let text = match value_to_text(value) {
        Some(text) => text,
        None => "NULL".to_string(),
    };
    markdown_cell(&text)
}

pub fn supports_sql_literals(driver_id: &str) -> bool {
    matches!(
        driver_id,
        "postgres" | "mysql" | "sqlite" | "mssql" | "clickhouse" | "duckdb"
    )
}

fn in_clause_literal(driver_id: &str, value: &Value) -> Option<String> {
    if !supports_sql_literals(driver_id) || matches!(value, Value::Null | Value::Bytes(_)) {
        return None;
    }
    if matches!(value, Value::Float(number) if !number.is_finite()) {
        return None;
    }
    if let Value::Bool(value) = value {
        return Some(
            match (driver_id, value) {
                ("mssql", true) => "1",
                ("mssql", false) => "0",
                (_, true) => "TRUE",
                (_, false) => "FALSE",
            }
            .into(),
        );
    }
    Some(crate::sql_literal::render_sql_literal(driver_id, value))
}

/// Escape and write a CSV header line for `columns`.
pub fn write_csv_header(w: &mut impl Write, columns: &[ColumnInfo]) -> io::Result<()> {
    for (i, col) in columns.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        write!(w, "{}", csv_escape(&col.name))?;
    }
    writeln!(w)?;
    Ok(())
}

/// Escape and write one CSV data row.
pub fn write_csv_row(w: &mut impl Write, row: &[Value]) -> io::Result<()> {
    for (i, cell) in row.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        write!(w, "{}", csv_escape(&value_to_csv_text(cell)))?;
    }
    writeln!(w)?;
    Ok(())
}

/// Legacy best-effort full-table CSV paging helper.
///
/// This does not establish a database snapshot or stable row order. It is not
/// used by the RC GUI; callers must provide those guarantees themselves before
/// presenting its output as a consistent full-table export.
/// Returns the number of data rows written.
pub async fn stream_table_to_csv(
    conn: &dyn Connection,
    schema: Option<&str>,
    table: &str,
    writer: &mut impl Write,
    page_size: u64,
) -> Result<u64, DriverError> {
    let page_size = page_size.max(1);
    let mut offset: u64 = 0;
    let mut total: u64 = 0;
    let mut header_written = false;

    loop {
        let page = conn.fetch_rows(schema, table, offset, page_size).await?;
        if !header_written {
            write_csv_header(writer, &page.columns).map_err(|e| DriverError::Internal(e.to_string()))?;
            header_written = true;
        }
        if page.rows.is_empty() {
            break;
        }
        for row in &page.rows {
            write_csv_row(writer, row).map_err(|e| DriverError::Internal(e.to_string()))?;
            total += 1;
        }
        if (page.rows.len() as u64) < page_size {
            break;
        }
        offset = offset.saturating_add(page_size);
    }
    Ok(total)
}

/// Stream rows from an arbitrary SELECT by re-running with LIMIT/OFFSET
/// pages. This legacy helper does not establish a snapshot or stable order;
/// the GUI exports its already loaded result instead.
pub async fn stream_query_to_csv(
    conn: &dyn Connection,
    driver_id: &str,
    base_sql: &str,
    writer: &mut impl Write,
    page_size: u64,
) -> Result<u64, DriverError> {
    let page_size = page_size.max(1);
    let mut offset: u64 = 0;
    let mut total: u64 = 0;
    let mut header_written = false;

    loop {
        let sql = format!(
            "{base_sql}{}",
            build_order_and_pagination(driver_id, None, page_size, offset)
        );
        let page = conn.query(&sql).await?;
        if !header_written {
            write_csv_header(writer, &page.columns).map_err(|e| DriverError::Internal(e.to_string()))?;
            header_written = true;
        }
        if page.rows.is_empty() {
            break;
        }
        for row in &page.rows {
            write_csv_row(writer, row).map_err(|e| DriverError::Internal(e.to_string()))?;
            total += 1;
        }
        if (page.rows.len() as u64) < page_size || page.truncated {
            break;
        }
        offset = offset.saturating_add(page_size);
    }
    Ok(total)
}

/// Write `path` so a reader never observes a half-written export. The
/// content goes to a sibling temporary file, is flushed and synced, then
/// renamed over the destination, which is atomic within a filesystem.
/// Writing straight to the destination meant another process could open
/// the file after the header and a few rows had reached the disk and read
/// a truncated export.
pub fn write_atomically<F>(path: &Path, fill: F) -> io::Result<()>
where
    F: FnOnce(&mut dyn Write) -> io::Result<()>,
{
    write_atomically_checked(path, fill, || Ok(()))
}

fn write_atomically_checked<F>(path: &Path, fill: F, before_publish: impl FnOnce() -> io::Result<()>) -> io::Result<()>
where
    F: FnOnce(&mut dyn Write) -> io::Result<()>,
{
    let directory = match path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        Some(parent) => parent,
        None => Path::new("."),
    };
    let mut temporary = tempfile::Builder::new()
        .prefix(".tablepro-export-")
        .tempfile_in(directory)?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        fill(&mut writer)?;
        writer.flush()?;
    }
    temporary.as_file().sync_all()?;
    before_publish()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn value_to_csv_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Text(s) => s.clone(),
        Value::Bytes(b) => {
            let mut s = String::with_capacity(2 + b.len() * 2);
            s.push_str("\\x");
            for byte in b {
                use std::fmt::Write as _;
                let _ = write!(s, "{byte:02x}");
            }
            s
        }
        Value::Date(d) => d.to_string(),
        Value::Time(t) => t.to_string(),
        Value::DateTime(dt) => dt.to_string(),
        Value::TimestampTz(ts) => ts.to_rfc3339(),
        Value::Decimal(d) => d.to_string(),
        Value::Uuid(u) => u.to_string(),
        Value::Json(j) => j.to_string(),
        Value::Undecodable(type_name) => undecodable_marker(type_name),
    }
}

#[cfg(test)]
mod atomic_write_tests {
    use super::*;

    #[test]
    fn overlapping_exports_publish_complete_independent_files() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("page.csv");
        fs::write(&path, "original").expect("existing export");

        write_atomically(&path, |outer| {
            outer.write_all(b"outer export")?;
            write_atomically(&path, |inner| inner.write_all(b"inner"))?;
            assert_eq!(fs::read_to_string(&path)?, "inner");
            Ok(())
        })
        .expect("both exports complete");

        assert_eq!(fs::read_to_string(&path).expect("read export"), "outer export");
        assert_eq!(fs::read_dir(directory.path()).expect("list").count(), 1);
    }

    #[test]
    fn a_failed_export_preserves_an_existing_destination() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("page.csv");
        fs::write(&path, "original").expect("existing export");
        assert!(
            write_atomically(&path, |writer| {
                writer.write_all(b"replacement")?;
                Err(io::Error::other("export failed"))
            })
            .is_err()
        );
        assert_eq!(fs::read_to_string(&path).expect("read export"), "original");
        assert_eq!(fs::read_dir(directory.path()).expect("list").count(), 1);
    }

    #[test]
    fn export_staging_stays_on_the_destination_filesystem() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("page.csv");
        write_atomically(&path, |writer| {
            // Atomic rename requires staging beside the destination, even
            // when the destination is on a different mount from the cwd.
            assert_eq!(fs::read_dir(directory.path())?.count(), 1);
            assert!(!path.exists());
            writer.write_all(b"complete export")
        })
        .expect("export");
        assert_eq!(fs::read_to_string(path).expect("read export"), "complete export");
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_temporary_symlink_cannot_redirect_an_export() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("page.csv");
        let unrelated = directory.path().join("unrelated.txt");
        fs::write(&unrelated, "keep me").expect("unrelated file");
        std::os::unix::fs::symlink(&unrelated, directory.path().join("page.csv.tablepro-part"))
            .expect("preexisting symlink");

        write_atomically(&path, |writer| writer.write_all(b"export")).expect("export");

        assert_eq!(fs::read_to_string(&unrelated).expect("unrelated file"), "keep me");
        assert_eq!(fs::read_to_string(&path).expect("export file"), "export");
        assert!(!path.is_symlink());
    }

    #[test]
    fn a_completed_write_lands_at_the_destination_and_leaves_no_leftovers() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("page.csv");

        write_atomically(&path, |writer| writer.write_all(b"id\n1\n2\n")).expect("write the export");

        assert_eq!(fs::read_to_string(&path).expect("read back"), "id\n1\n2\n");
        assert_eq!(
            fs::read_dir(directory.path()).expect("list").count(),
            1,
            "the temporary file must not survive"
        );
    }

    #[test]
    fn the_destination_never_exists_while_the_export_is_still_being_written() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("page.csv");
        let observed = std::cell::Cell::new(false);

        write_atomically(&path, |writer| {
            writer.write_all(b"id\n")?;
            for row in 0..500 {
                writeln!(writer, "{row}")?;
            }
            observed.set(path.exists());
            Ok(())
        })
        .expect("write the export");

        assert!(
            !observed.get(),
            "a reader must not be able to open a partially written export"
        );
        assert!(path.exists(), "the finished export must exist");
    }

    #[test]
    fn a_failed_write_leaves_neither_the_destination_nor_a_temporary_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("page.csv");

        let error = write_atomically(&path, |writer| {
            writer.write_all(b"id\n")?;
            Err(io::Error::other("driver stopped mid-export"))
        })
        .expect_err("the failure must be reported");

        assert_eq!(error.to_string(), "driver stopped mid-export");
        assert!(!path.exists(), "a failed export must not leave a destination file");
        assert_eq!(
            fs::read_dir(directory.path()).expect("list").count(),
            0,
            "a failed export must not leave a temporary file"
        );
    }

    #[test]
    fn a_second_export_replaces_the_first_without_a_partial_state() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("page.csv");

        write_atomically(&path, |writer| writer.write_all(b"first")).expect("first export");
        write_atomically(&path, |writer| writer.write_all(b"second")).expect("second export");

        assert_eq!(fs::read_to_string(&path).expect("read back"), "second");
        assert_eq!(fs::read_dir(directory.path()).expect("list").count(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undecodable_value_renders_as_a_visible_marker_not_an_empty_null() {
        let value = Value::Undecodable("NUMERIC".into());
        assert_eq!(value_to_text(&value), Some("<undecodable NUMERIC>".into()));
        assert_eq!(value_to_csv_text(&value), "<undecodable NUMERIC>");
    }

    #[test]
    fn csv_header_and_row() {
        let cols = vec![ColumnInfo {
            name: "a,b".into(),
            data_type: "text".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
        }];
        let mut buf = Vec::new();
        write_csv_header(&mut buf, &cols).unwrap();
        write_csv_row(&mut buf, &[Value::Text("hello".into())]).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s, "\"a,b\"\nhello\n");
    }

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
        }
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

    #[test]
    fn json_renderer_keeps_value_types_duplicate_columns_and_missing_cells() {
        let columns = vec![column("id"), column("id"), column("id_2"), column("payload")];
        let rows = vec![
            vec![
                Value::Int(7),
                Value::Float(1.5),
                Value::Decimal("12.30".parse().unwrap()),
                Value::Json(serde_json::json!({"ok": true})),
            ],
            vec![Value::Int(8)],
        ];

        assert_eq!(json_field_names(&columns), vec!["id", "id_3", "id_2", "payload"]);
        assert_eq!(
            render_json(&columns, &rows),
            "[\n  {\n    \"id\": 7,\n    \"id_3\": 1.5,\n    \"id_2\": 12.30,\n    \"payload\": {\n      \"ok\": true\n    }\n  },\n  {\n    \"id\": 8,\n    \"id_3\": null,\n    \"id_2\": null,\n    \"payload\": null\n  }\n]"
        );
    }

    #[test]
    fn markdown_renderer_escapes_pipes_and_preserves_line_breaks() {
        let columns = vec![column("a|b"), column("empty")];
        let rows = vec![vec![Value::Text("first\r\nsecond|third".into()), Value::Null]];

        assert_eq!(
            render_markdown(&columns, &rows),
            "| a\\|b | empty |\n| --- | --- |\n| first<br>second\\|third | NULL |"
        );
    }

    #[test]
    fn in_clause_renderer_quotes_values_and_reports_unusable_values() {
        let rows = vec![
            vec![Value::Int(1)],
            vec![Value::Text("O'Reilly".into())],
            vec![Value::Bool(true)],
            vec![Value::Null],
            vec![Value::Bytes(vec![0])],
        ];

        assert_eq!(
            render_in_clause("postgres", &rows, 0),
            InClause {
                sql: "(1, 'O''Reilly', TRUE)".into(),
                skipped: 2
            }
        );
        assert_eq!(render_in_clause("postgres", &rows, 1), InClause::default());
    }

    #[test]
    fn in_clause_uses_dialect_escaping_and_preserves_fractional_time() {
        let rows = vec![vec![Value::Text("x\\' OR 1=1 -- ".into())]];
        for driver in ["mysql", "clickhouse"] {
            assert_eq!(render_in_clause(driver, &rows, 0).sql, "('x\\\\'' OR 1=1 -- ')");
        }
        assert_eq!(render_in_clause("postgres", &rows, 0).sql, "('x\\'' OR 1=1 -- ')");
        assert!(render_in_clause("redis", &rows, 0).sql.is_empty());
        assert_eq!(render_in_clause("redis", &rows, 0).skipped, 1);
        let time = Value::Time("12:34:56.123456".parse().unwrap());
        assert_eq!(value_to_text(&time).as_deref(), Some("12:34:56.123456"));
        assert_eq!(
            render_in_clause("postgres", &[vec![time]], 0).sql,
            "('12:34:56.123456')"
        );
    }

    #[test]
    fn value_to_text_preserves_non_json_value_representations() {
        assert_eq!(value_to_text(&Value::Bytes(vec![0xde, 0xad])), Some("\\xdead".into()));
        assert_eq!(value_to_text(&Value::Null), None);
        assert_eq!(
            value_to_text(&Value::Json(serde_json::json!({"a": 1}))),
            Some("{\"a\":1}".into())
        );
    }
}
