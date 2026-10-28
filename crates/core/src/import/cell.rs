use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use rust_decimal::Decimal;
use thiserror::Error;
use uuid::Uuid;

use crate::import::csv_import::CsvImportOptions;
use crate::import::extended_temporal::{duckdb_extended_date, duckdb_extended_timestamptz_nanos};
use crate::import::integer_type::is_integer_type_name;
use crate::query::{ColumnInfo, Value};

mod mysql_calendar;
mod postgres_temporal;

/// What a field could not be turned into. Names the failure only: the cell
/// text never travels with it, because this reaches error lists and logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CellError {
    #[error("not true or false")]
    NotABoolean,
    #[error("not a whole number")]
    NotAnInteger,
    #[error("not a number")]
    NotANumber,
    #[error("not a date")]
    NotADate,
    #[error("not a time")]
    NotATime,
    #[error("not a timestamp")]
    NotATimestamp,
    #[error("not a UUID")]
    NotAUuid,
    #[error("not JSON")]
    NotJson,
    #[error("not hexadecimal bytes")]
    NotBytes,
    #[error("not an interval")]
    NotAnInterval,
    #[error("empty CSV field is ambiguous for a PostgreSQL enum; set an explicit NULL marker")]
    AmbiguousEnumNullOrEmpty,
    #[error("empty CSV field is ambiguous for SQLite ANY; set an explicit NULL marker")]
    AmbiguousSqliteAnyNullOrEmpty,
    #[error("invalid tagged SQLite ANY CSV value")]
    InvalidSqliteAnyCsvValue,
}

/// A field that could not become a value, named well enough for the user
/// to find the cell without the message carrying its contents.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("line {line}, column {column}: {reason}")]
pub struct CsvRowError {
    /// The row's position in the file, counting the header as line 1 so it
    /// matches what a spreadsheet shows.
    pub line: usize,
    pub column: String,
    pub reason: CellError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Bool,
    Int,
    Float,
    Decimal,
    Date,
    Time,
    DateTime,
    TimestampTz,
    Uuid,
    Json,
    Bytes,
    Text,
}

impl ColumnKind {
    pub const ALL: [Self; 12] = [
        Self::Text,
        Self::Int,
        Self::Float,
        Self::Decimal,
        Self::Bool,
        Self::Date,
        Self::Time,
        Self::DateTime,
        Self::TimestampTz,
        Self::Uuid,
        Self::Json,
        Self::Bytes,
    ];
}

/// Read a catalog type name as the value shape the import must produce.
/// Anything unrecognised is text, which is the reading that never loses
/// what the file said.
pub fn column_kind(data_type: &str) -> ColumnKind {
    let lowered = data_type.trim().to_ascii_lowercase();
    if lowered.starts_with("bindata-subtype-") {
        return if lowered.ends_with("-00") {
            ColumnKind::Bytes
        } else {
            ColumnKind::Json
        };
    }
    // Array values are transported as their driver's textual array form.
    // Parsing the element type here would incorrectly treat the whole cell
    // (for example, "{1,2}") as a scalar number.
    if lowered.contains('[') || lowered.ends_with(" array") {
        return ColumnKind::Text;
    }
    let base = lowered.split(['(', '[']).next().unwrap_or("").trim().to_owned();
    if let Some(kind) = temporal_kind(&base, &lowered) {
        return kind;
    }
    if base.contains("bool") || base == "bit" {
        return ColumnKind::Bool;
    }
    if base.contains("uuid") || base.contains("uniqueidentifier") {
        return ColumnKind::Uuid;
    }
    if matches!(base.as_str(), "int128" | "uint128" | "int256" | "uint256") {
        return ColumnKind::Text;
    }
    if base.contains("json") {
        return ColumnKind::Json;
    }
    if base.contains("bytea") || base.contains("blob") || base.contains("binary") || base == "image" {
        return ColumnKind::Bytes;
    }
    if base.contains("decimal") || base.contains("numeric") || base.contains("money") {
        return ColumnKind::Decimal;
    }
    if base.split_whitespace().any(is_integer_type_name) {
        return ColumnKind::Int;
    }
    if base.contains("float") || base.contains("double") || base.contains("real") {
        return ColumnKind::Float;
    }
    ColumnKind::Text
}

fn temporal_kind(base: &str, lowered: &str) -> Option<ColumnKind> {
    // SQL Server datetimeoffset stores the original UTC offset as part of the
    // value. Value::TimestampTz normalizes offsets to UTC, so CSV import must
    // retain this native type as exact text for a lossless bound insert.
    if base.contains("datetimeoffset") {
        return Some(ColumnKind::Text);
    }
    if base.contains("timestamptz") || lowered.contains("with time zone") {
        return Some(ColumnKind::TimestampTz);
    }
    if base.contains("timestamp") || base.contains("datetime") {
        return Some(ColumnKind::DateTime);
    }
    if base == "date" {
        return Some(ColumnKind::Date);
    }
    if base.contains("time") {
        return Some(ColumnKind::Time);
    }
    None
}

pub fn parse_cell(text: &str, kind: ColumnKind) -> Result<Value, CellError> {
    let trimmed = text.trim();
    match kind {
        ColumnKind::Text => Ok(Value::Text(text.to_owned())),
        ColumnKind::Bool => parse_bool(trimmed),
        ColumnKind::Int => trimmed.parse().map(Value::Int).map_err(|_| CellError::NotAnInteger),
        ColumnKind::Float => crate::parse_float_input(trimmed)
            .map(Value::Float)
            .map_err(|_| CellError::NotANumber),
        ColumnKind::Decimal => Decimal::from_str_exact(trimmed)
            .map(Value::Decimal)
            .map_err(|_| CellError::NotANumber),
        ColumnKind::Uuid => Uuid::parse_str(trimmed)
            .map(Value::Uuid)
            .map_err(|_| CellError::NotAUuid),
        ColumnKind::Json => serde_json::from_str(trimmed)
            .map(Value::Json)
            .map_err(|_| CellError::NotJson),
        ColumnKind::Bytes => parse_bytes(trimmed),
        ColumnKind::Date => NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
            .map(Value::Date)
            .map_err(|_| CellError::NotADate),
        ColumnKind::Time => parse_time(trimmed),
        ColumnKind::DateTime => parse_datetime(trimmed).map(Value::DateTime),
        ColumnKind::TimestampTz => parse_timestamptz(trimmed),
    }
}

fn parse_bool(text: &str) -> Result<Value, CellError> {
    match text.to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "1" => Ok(Value::Bool(true)),
        "false" | "f" | "no" | "n" | "0" => Ok(Value::Bool(false)),
        _ => Err(CellError::NotABoolean),
    }
}

const TIME_FORMATS: [&str; 2] = ["%H:%M:%S%.f", "%H:%M"];

fn parse_time(text: &str) -> Result<Value, CellError> {
    TIME_FORMATS
        .iter()
        .find_map(|format| NaiveTime::parse_from_str(text, format).ok())
        .map(Value::Time)
        .ok_or(CellError::NotATime)
}

const DATETIME_FORMATS: [&str; 4] = [
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M",
    "%Y-%m-%dT%H:%M",
];

fn parse_datetime(text: &str) -> Result<NaiveDateTime, CellError> {
    if let Some(parsed) = DATETIME_FORMATS
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
    {
        return Ok(parsed);
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .ok_or(CellError::NotATimestamp)
}

fn parse_timestamptz(text: &str) -> Result<Value, CellError> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Ok(Value::TimestampTz(parsed.with_timezone(&Utc)));
    }
    parse_datetime(text).map(|naive| Value::TimestampTz(naive.and_utc()))
}

/// Binary columns take hexadecimal, with or without the `0x` prefix every
/// engine's own hex literal uses. There is no other text form a delimited
/// file can carry that round-trips through every driver.
fn parse_bytes(text: &str) -> Result<Value, CellError> {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .or_else(|| text.strip_prefix("\\x"))
        .unwrap_or(text);
    if !digits.len().is_multiple_of(2) {
        return Err(CellError::NotBytes);
    }
    let mut out = Vec::with_capacity(digits.len() / 2);
    for pair in digits.as_bytes().chunks(2) {
        let pair = std::str::from_utf8(pair).map_err(|_| CellError::NotBytes)?;
        out.push(u8::from_str_radix(pair, 16).map_err(|_| CellError::NotBytes)?);
    }
    Ok(Value::Bytes(out))
}

/// Turn one record into the values a bound INSERT takes, one per column in
/// `columns`. A column with no mapped field, or one whose field is past the
/// end of a short row, becomes NULL, which the statement builder drops when
/// the column has a server default.
pub fn row_to_values(
    row: &[String],
    mapping: &[Option<usize>],
    columns: &[ColumnInfo],
    options: &CsvImportOptions,
    line: usize,
) -> Result<Vec<Value>, CsvRowError> {
    row_to_values_for_driver(row, mapping, columns, options, line, "")
}

pub(crate) fn row_to_values_for_driver(
    row: &[String],
    mapping: &[Option<usize>],
    columns: &[ColumnInfo],
    options: &CsvImportOptions,
    line: usize,
    driver_id: &str,
) -> Result<Vec<Value>, CsvRowError> {
    columns
        .iter()
        .zip(mapping)
        .map(|(column, field)| {
            let Some(text) = field.and_then(|index| row.get(index)) else {
                return Ok(Value::Null);
            };
            value_for(text, column, options, driver_id).map_err(|reason| CsvRowError {
                line,
                column: column.name.clone(),
                reason,
            })
        })
        .collect()
}

fn value_for(text: &str, column: &ColumnInfo, options: &CsvImportOptions, driver_id: &str) -> Result<Value, CellError> {
    let kind = crate::import::text_kind::column_kind_for(column, driver_id);
    if driver_id == "sqlite" {
        let is_any = crate::sqlite_any_csv::is_any_type(&column.data_type);
        let is_unknown = crate::sqlite_any_csv::is_unknown_type(&column.data_type);
        if text == options.null_marker {
            return if is_any && options.null_marker.is_empty() {
                Err(CellError::AmbiguousSqliteAnyNullOrEmpty)
            } else {
                Ok(Value::Null)
            };
        }
        return match crate::sqlite_any_csv::decode(text) {
            Ok(Some(value)) if is_any || is_unknown => Ok(value),
            Ok(Some(Value::Int(value))) => parse_non_null_cell(&value.to_string(), column, kind, driver_id),
            Ok(Some(Value::Float(value))) => parse_non_null_cell(&value.to_string(), column, kind, driver_id),
            Ok(Some(Value::Text(value))) => parse_non_null_cell(&value, column, kind, driver_id),
            Ok(Some(Value::Bytes(value))) if matches!(kind, ColumnKind::Bytes) => Ok(Value::Bytes(value)),
            Ok(Some(Value::Bytes(_))) => Err(CellError::NotBytes),
            Ok(Some(_)) => Err(CellError::InvalidSqliteAnyCsvValue),
            Ok(None) if is_any => Ok(Value::Text(text.to_owned())),
            Ok(None) => parse_non_null_cell(text, column, kind, driver_id),
            Err(crate::sqlite_any_csv::DecodeError::Malformed) => Err(CellError::InvalidSqliteAnyCsvValue),
        };
    }
    if driver_id == "postgres" && column.enum_type.is_some() && text.is_empty() && options.null_marker.is_empty() {
        return if column.data_type.trim().ends_with("[]") {
            Ok(Value::Null)
        } else {
            Err(CellError::AmbiguousEnumNullOrEmpty)
        };
    }
    if driver_id == "duckdb" && column.data_type.trim().eq_ignore_ascii_case("interval") && text.is_empty() {
        return if options.null_marker.is_empty() {
            Ok(Value::Null)
        } else {
            Err(CellError::NotAnInterval)
        };
    }
    if text != options.null_marker {
        return parse_non_null_cell(text, column, kind, driver_id);
    }
    // Every export in this app writes NULL as an empty field, but so is an
    // empty string, and for text the empty string is the reading that loses
    // nothing.
    if options.null_marker.is_empty() && kind == ColumnKind::Text {
        return Ok(Value::Text(String::new()));
    }
    Ok(Value::Null)
}

fn parse_non_null_cell(text: &str, column: &ColumnInfo, kind: ColumnKind, driver_id: &str) -> Result<Value, CellError> {
    if driver_id == "sqlite"
        && kind == ColumnKind::Text
        && crate::sqlite_declared_type_has_numeric_affinity(&column.data_type)
    {
        return sqlite_affinity_csv_value(text);
    }
    if driver_id == "duckdb"
        && let Some(value) = duckdb_extended_calendar_text(text, &column.data_type)
    {
        return Ok(Value::Text(value.to_owned()));
    }
    if driver_id == "duckdb"
        && let Some(value) = duckdb_formula_safe_interval(text, &column.data_type)
    {
        return Ok(Value::Text(value.to_owned()));
    }
    if kind == ColumnKind::Text
        && let Some(value) = sanitized_wide_integer_text(text, &column.data_type)
    {
        return Ok(Value::Text(value.to_owned()));
    }
    if kind == ColumnKind::Decimal
        && let Some(value) = sanitized_decimal(text)
    {
        return Ok(Value::Decimal(value));
    }
    if kind == ColumnKind::Decimal
        && let Some(value) = wide_decimal_text(text, &column.data_type)
    {
        return Ok(Value::Text(value.to_owned()));
    }
    if driver_id == "mysql" && mysql_calendar::invalid_text(text, &column.data_type) {
        return Ok(Value::Text(text.to_owned()));
    }
    if driver_id == "postgres"
        && let Some(value) = postgres_temporal::import_override(text, &column.data_type, kind)
    {
        return Ok(value);
    }
    match parse_cell(text, kind) {
        Ok(Value::TimestampTz(value)) if driver_id == "duckdb" && value.timestamp_subsec_nanos() % 1_000 != 0 => {
            Err(CellError::NotATimestamp)
        }
        Ok(value) if driver_id == "postgres" && postgres_temporal::needs_text(&value) => {
            match crate::sql_literal::postgres_temporal_text(&value) {
                Some(text) => Ok(Value::Text(text)),
                None => Ok(value),
            }
        }
        Ok(value) => Ok(value),
        Err(error) => parse_sqlite_affinity_fallback(text, kind, driver_id).unwrap_or(Err(error)),
    }
}

fn sqlite_affinity_csv_value(text: &str) -> Result<Value, CellError> {
    if let Some(decimal) = crate::sqlite_affinity_decimal(text) {
        Ok(Value::Decimal(decimal))
    } else if crate::is_numeric_input(text) {
        Err(CellError::NotANumber)
    } else {
        Ok(Value::Text(text.to_owned()))
    }
}

fn parse_sqlite_affinity_fallback(text: &str, kind: ColumnKind, driver_id: &str) -> Option<Result<Value, CellError>> {
    if driver_id != "sqlite" || !matches!(kind, ColumnKind::Int | ColumnKind::Float | ColumnKind::Decimal) {
        return None;
    }
    if let Some(decimal) = crate::sqlite_affinity_decimal(text) {
        return Some(Ok(Value::Decimal(decimal)));
    }
    if !crate::is_numeric_input(text) {
        // SQLite affinity may store nonnumeric input as TEXT.
        return Some(Ok(Value::Text(text.to_owned())));
    }
    None
}

fn duckdb_extended_calendar_text<'a>(text: &'a str, data_type: &str) -> Option<&'a str> {
    let data_type = data_type.trim();
    let date_text = if data_type.eq_ignore_ascii_case("date") {
        text
    } else if ["timestamptz", "timestamp with time zone"]
        .iter()
        .any(|kind| data_type.eq_ignore_ascii_case(kind))
    {
        if duckdb_extended_timestamptz_nanos(text)? % 1_000 != 0 {
            return None;
        }
        return Some(text);
    } else if ["timestamp", "timestamp without time zone"]
        .iter()
        .any(|kind| data_type.eq_ignore_ascii_case(kind))
    {
        let (date, time) = if let Some((date, time)) = text.split_once(" (BC) ") {
            (format!("{date} (BC)"), time)
        } else {
            let (date, time) = text.split_once(' ')?;
            (date.to_owned(), time)
        };
        let valid_date = duckdb_extended_date(&date);
        if !valid_date {
            return None;
        }
        let time = NaiveTime::parse_from_str(time, "%H:%M:%S%.f")
            .or_else(|_| NaiveTime::parse_from_str(time, "%H:%M"))
            .ok()?;
        if time.nanosecond() % 1_000 != 0 {
            return None;
        }
        return Some(text);
    } else {
        return None;
    };
    duckdb_extended_date(date_text).then_some(text)
}

fn duckdb_formula_safe_interval<'a>(text: &'a str, data_type: &str) -> Option<&'a str> {
    if !data_type.trim().eq_ignore_ascii_case("interval") {
        return None;
    }
    let value = text.strip_prefix('\'')?;
    if !valid_duckdb_interval_text(value) {
        return None;
    }
    Some(value)
}

fn valid_duckdb_interval_text(value: &str) -> bool {
    let mut fields = value.split_whitespace();
    let Some(months) = fields.next().and_then(|value| value.parse::<i32>().ok()) else {
        return false;
    };
    let Some(month_unit) = fields.next() else {
        return false;
    };
    let Some(days) = fields.next().and_then(|value| value.parse::<i32>().ok()) else {
        return false;
    };
    let Some(day_unit) = fields.next() else {
        return false;
    };
    let Some(microseconds) = fields.next().and_then(|value| value.parse::<i64>().ok()) else {
        return false;
    };
    let Some(microsecond_unit) = fields.next() else {
        return false;
    };
    fields.next().is_none()
        && month_unit == if months.unsigned_abs() == 1 { "month" } else { "months" }
        && day_unit == if days.unsigned_abs() == 1 { "day" } else { "days" }
        && microsecond_unit
            == if microseconds.unsigned_abs() == 1 {
                "microsecond"
            } else {
                "microseconds"
            }
}

fn sanitized_wide_integer_text<'a>(text: &'a str, data_type: &str) -> Option<&'a str> {
    let value = text.strip_prefix('\'')?;
    if !value.starts_with(['+', '-']) {
        return None;
    }
    if data_type.trim().eq_ignore_ascii_case("Int128") && value.parse::<i128>().is_ok() {
        return Some(value);
    }
    if data_type.trim().eq_ignore_ascii_case("UInt128") && value.parse::<u128>().is_ok() {
        return Some(value);
    }
    None
}

fn sanitized_decimal(text: &str) -> Option<Decimal> {
    Decimal::from_str_exact(text.strip_prefix('\'')?).ok()
}

fn wide_decimal_text<'a>(text: &'a str, data_type: &str) -> Option<&'a str> {
    let value = text.strip_prefix('\'').unwrap_or(text);
    if text.starts_with('\'') && !value.starts_with(['+', '-']) {
        return None;
    }
    let (precision, scale) = decimal_type_limits(data_type)?;
    if precision <= 28 && scale <= 28 {
        return None;
    }

    let unsigned = value.strip_prefix(['+', '-']).unwrap_or(value);
    let (integer, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if integer.is_empty() && fraction.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > scale
    {
        return None;
    }
    let significant_integer_digits = integer.trim_start_matches('0').len();
    if significant_integer_digits > precision.saturating_sub(scale) {
        return None;
    }
    Some(value)
}

fn decimal_type_limits(data_type: &str) -> Option<(usize, usize)> {
    let lowered = data_type.trim().to_ascii_lowercase();
    let base = lowered.split('(').next()?.trim();
    if !matches!(base, "decimal" | "numeric" | "dec") {
        return None;
    }
    let (arguments, suffix) = lowered.split_once('(')?.1.split_once(')')?;
    let mut saw_unsigned = false;
    let mut saw_zerofill = false;
    for modifier in suffix.split_whitespace() {
        match modifier {
            "unsigned" if !saw_unsigned => saw_unsigned = true,
            "zerofill" if !saw_zerofill => saw_zerofill = true,
            _ => return None,
        }
    }
    let mut parts = arguments.split(',').map(str::trim);
    let precision = parts.next()?.parse().ok()?;
    let scale = parts.next().map_or(Some(0), |part| part.parse().ok())?;
    if parts.next().is_some() || scale > precision {
        return None;
    }
    Some((precision, scale))
}

#[cfg(test)]
#[path = "cell_tests.rs"]
mod tests;
