use crate::import::cell::{ColumnKind, parse_cell};
use crate::import::csv_import::{CsvImportOptions, CsvSheet};
use crate::sql_ddl::DraftColumn;
use crate::sql_dialect::MAX_IDENT_BYTES;

/// Rows the type guess reads. A guess from a bounded sample is a guess,
/// which is why the dialog shows it and lets the user change it.
pub const INFER_SAMPLE_ROWS: usize = 200;

/// Narrowest to widest. A field takes the first kind every sampled value
/// fits; text fits everything and is the answer when nothing else does.
const CANDIDATES: [ColumnKind; 7] = [
    ColumnKind::Bool,
    ColumnKind::Int,
    ColumnKind::Decimal,
    ColumnKind::Date,
    ColumnKind::DateTime,
    ColumnKind::TimestampTz,
    ColumnKind::Uuid,
];

/// Guess one draft column per field in the file. Every column is
/// nullable and has no default: an import decides what a table holds, not
/// what it requires.
pub fn infer_columns(sheet: &CsvSheet, options: &CsvImportOptions, driver_id: &str) -> Vec<DraftColumn> {
    let mut names = Vec::with_capacity(sheet.headers.len());
    sheet
        .headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            let name = unique_name(column_name(header, index), &mut names);
            DraftColumn {
                original: None,
                name,
                data_type: type_name(driver_id, infer_kind(sheet, index, options)).to_owned(),
                nullable: true,
                primary_key: false,
                auto_increment: false,
                default_value: None,
                comment: None,
                collation: None,
            }
        })
        .collect()
}

fn infer_kind(sheet: &CsvSheet, field: usize, options: &CsvImportOptions) -> ColumnKind {
    let sample: Vec<&str> = sheet
        .rows
        .iter()
        .take(INFER_SAMPLE_ROWS)
        .filter_map(|row| row.get(field))
        .map(String::as_str)
        .filter(|text| !text.trim().is_empty() && *text != options.null_marker)
        .collect();
    if sample.is_empty() {
        return ColumnKind::Text;
    }
    CANDIDATES
        .into_iter()
        .find(|kind| sample.iter().all(|text| fits(text, *kind)))
        .unwrap_or(ColumnKind::Text)
}

/// `0` and `1` are both a boolean and a whole number. The value parser
/// takes either because the column already said which it is; a guess has
/// no such column, so here only the written words count as a boolean and
/// a file of ones and zeroes stays a number.
fn fits(text: &str, kind: ColumnKind) -> bool {
    if kind == ColumnKind::Bool {
        return matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "true" | "false" | "t" | "f" | "yes" | "no" | "y" | "n"
        );
    }
    if matches!(kind, ColumnKind::Int | ColumnKind::Decimal) && is_formatted_identifier(text.trim()) {
        return false;
    }
    parse_cell(text, kind).is_ok()
}

fn is_formatted_identifier(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let mut bytes = digits.bytes();
    digits.starts_with('+') || (bytes.next() == Some(b'0') && bytes.next().is_some_and(|next| next.is_ascii_digit()))
}

/// A header is untrusted text. Strip what no identifier may carry, fall
/// back to a positional name when nothing usable is left, and keep it
/// inside the identifier length limit.
fn column_name(header: &str, index: usize) -> String {
    let cleaned: String = header
        .trim()
        .chars()
        .filter(|character| !character.is_control() && *character != '\u{2028}' && *character != '\u{2029}')
        .collect();
    let cleaned = cleaned.trim().to_owned();
    if cleaned.is_empty() {
        return format!("column_{}", index + 1);
    }
    truncate_to(cleaned, MAX_IDENT_BYTES)
}

fn truncate_to(mut name: String, limit: usize) -> String {
    while name.len() > limit {
        name.pop();
    }
    name
}

fn unique_name(name: String, taken: &mut Vec<String>) -> String {
    let mut candidate = name.clone();
    let mut suffix = 2;
    while taken.iter().any(|existing| existing.eq_ignore_ascii_case(&candidate)) {
        let suffix_text = format!("_{suffix}");
        let prefix = truncate_to(name.clone(), MAX_IDENT_BYTES.saturating_sub(suffix_text.len()));
        candidate = format!("{prefix}{suffix_text}");
        suffix += 1;
    }
    taken.push(candidate.clone());
    candidate
}

/// The engine's own spelling for a value shape. Unknown engines take the
/// ANSI-leaning set, which every supported engine parses.
pub fn supports_table_creation(driver_id: &str) -> bool {
    matches!(driver_id, "postgres" | "mysql" | "sqlite" | "mssql" | "duckdb")
}

pub fn type_name(driver_id: &str, kind: ColumnKind) -> &'static str {
    match driver_id {
        "duckdb" if kind == ColumnKind::Json => "JSON",
        "sqlite" => sqlite_type(kind),
        "mysql" => mysql_type(kind),
        "mssql" => mssql_type(kind),
        _ => postgres_type(kind),
    }
}

fn postgres_type(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Bool => "BOOLEAN",
        ColumnKind::Int => "BIGINT",
        ColumnKind::Float => "DOUBLE PRECISION",
        ColumnKind::Decimal => "NUMERIC",
        ColumnKind::Date => "DATE",
        ColumnKind::Time => "TIME",
        ColumnKind::DateTime => "TIMESTAMP",
        ColumnKind::TimestampTz => "TIMESTAMPTZ",
        ColumnKind::Uuid => "UUID",
        ColumnKind::Json => "JSONB",
        ColumnKind::Bytes => "BYTEA",
        ColumnKind::Text => "TEXT",
    }
}

/// SQLite has no date, time, uuid or boolean storage class; those columns
/// hold the text the file carried and the driver reads them back as text.
fn sqlite_type(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Bool | ColumnKind::Int => "INTEGER",
        ColumnKind::Float => "REAL",
        ColumnKind::Decimal => "NUMERIC",
        ColumnKind::Bytes => "BLOB",
        _ => "TEXT",
    }
}

fn mysql_type(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Bool => "BOOLEAN",
        ColumnKind::Int => "BIGINT",
        ColumnKind::Float => "DOUBLE",
        ColumnKind::Decimal => "DECIMAL(38,10)",
        ColumnKind::Date => "DATE",
        ColumnKind::Time => "TIME",
        ColumnKind::DateTime => "DATETIME",
        ColumnKind::TimestampTz => "TIMESTAMP",
        ColumnKind::Uuid => "CHAR(36)",
        ColumnKind::Json => "JSON",
        ColumnKind::Bytes => "BLOB",
        ColumnKind::Text => "TEXT",
    }
}

fn mssql_type(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Bool => "BIT",
        ColumnKind::Int => "BIGINT",
        ColumnKind::Float => "FLOAT",
        ColumnKind::Decimal => "DECIMAL(38,10)",
        ColumnKind::Date => "DATE",
        ColumnKind::Time => "TIME",
        ColumnKind::DateTime => "DATETIME2",
        ColumnKind::TimestampTz => "DATETIMEOFFSET",
        ColumnKind::Uuid => "UNIQUEIDENTIFIER",
        ColumnKind::Json | ColumnKind::Text => "NVARCHAR(MAX)",
        ColumnKind::Bytes => "VARBINARY(MAX)",
    }
}

#[cfg(test)]
#[path = "infer_tests.rs"]
mod tests;
