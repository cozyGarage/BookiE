use tablepro_core::{ColumnInfo, Value};

/// Collapse newlines / carriage returns to spaces, then squash any
/// resulting consecutive whitespace runs to a single space. Applied
/// at cell-edit commit time for non-JSON columns so a multi-line
/// clipboard paste into a single-line cell never reaches the SQL
/// layer with embedded `\n` — driver behaviour for that case is
/// type-specific (text columns store literally; numeric / date
/// columns parse-fail) and worth normalising up front.
pub(super) fn normalize_single_line_input(text: &str) -> String {
    let replaced: String = text
        .chars()
        .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
        .collect();
    replaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn parse_input_for_column(text: &str, col: Option<&ColumnInfo>) -> Result<Value, String> {
    let Some(col) = col else {
        return Ok(Value::Text(text.to_string()));
    };
    if text.is_empty() {
        if col.nullable || col.default_value.is_some() {
            return Ok(Value::Null);
        }
        return Err(crate::tr!("Field is required"));
    }
    let dt = col.data_type.to_ascii_lowercase();
    let trimmed = text.trim();
    if let Some(width) = mysql_bit_width(&dt) {
        return parse_mysql_bit_value(trimmed, width);
    }
    match classify_type(&dt) {
        TypeKind::Bool => parse_bool_value(trimmed),
        TypeKind::Int => parse_int_value(trimmed),
        TypeKind::Float => parse_float_value(trimmed),
        TypeKind::Decimal => parse_decimal_value(trimmed),
        TypeKind::Uuid => parse_uuid_value(trimmed),
        TypeKind::Json => parse_json_value(trimmed),
        TypeKind::TimestampTz => parse_timestamptz_value(trimmed),
        TypeKind::DateTime => parse_datetime_value(trimmed),
        TypeKind::Date => parse_date_value(trimmed),
        TypeKind::Time => parse_time_value(trimmed),
        TypeKind::Text => Ok(Value::Text(text.to_string())),
    }
}

pub(super) fn parse_input_for_driver(text: &str, col: Option<&ColumnInfo>, driver_id: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if driver_id == "postgres" && col.is_some_and(|column| is_postgres_numeric_type(&column.data_type)) {
        if matches!(trimmed, "NaN" | "Infinity" | "-Infinity") {
            return Ok(Value::Text(trimmed.into()));
        }
        return match parse_decimal_value(trimmed) {
            Ok(value) => Ok(value),
            Err(_) if is_postgres_numeric_literal(trimmed) => Ok(Value::Text(trimmed.into())),
            Err(error) => Err(error),
        };
    }
    if let Some(result) = parse_mongodb_decimal_input(trimmed, col, driver_id) {
        return result;
    }
    if let Some(result) = parse_mongodb_date_input(trimmed, col, driver_id) {
        return result;
    }
    if let Some(result) = parse_duckdb_timestamptz_input(trimmed, col, driver_id) {
        return result;
    }
    parse_input_for_column(text, col)
}

fn parse_mongodb_decimal_input(text: &str, col: Option<&ColumnInfo>, driver_id: &str) -> Option<Result<Value, String>> {
    let column = col?;
    if driver_id != "mongodb" || !column.data_type.trim().eq_ignore_ascii_case("decimal") {
        return None;
    }
    if let Ok(value) = parse_decimal_value(text) {
        return Some(Ok(value));
    }
    Some(
        drivers_mongodb::canonical_decimal128_text(text)
            .map(|canonical| Value::Json(serde_json::json!({"$numberDecimal": canonical})))
            .map_err(|_| crate::tr!("Invalid decimal")),
    )
}

fn parse_mongodb_date_input(text: &str, col: Option<&ColumnInfo>, driver_id: &str) -> Option<Result<Value, String>> {
    let column = col?;
    if driver_id != "mongodb" || !column.data_type.trim().eq_ignore_ascii_case("date") {
        return None;
    }
    let timestamp = chrono::DateTime::parse_from_rfc3339(text).ok()?;
    Some(if timestamp.timestamp_subsec_nanos() % 1_000_000 == 0 {
        Ok(Value::TimestampTz(timestamp.to_utc()))
    } else {
        Err(crate::tr!("MongoDB BSON dates support millisecond precision only"))
    })
}

fn is_postgres_numeric_type(data_type: &str) -> bool {
    let data_type = data_type.trim().to_ascii_lowercase();
    matches!(data_type.as_str(), "numeric" | "decimal")
        || data_type.starts_with("numeric(")
        || data_type.starts_with("decimal(")
}

fn is_duckdb_timestamptz_type(data_type: &str) -> bool {
    matches!(
        data_type.trim().to_ascii_lowercase().as_str(),
        "timestamptz" | "timestamp with time zone"
    )
}

fn parse_duckdb_timestamptz_input(
    text: &str,
    col: Option<&ColumnInfo>,
    driver_id: &str,
) -> Option<Result<Value, String>> {
    let column = col?;
    if driver_id != "duckdb" || !is_duckdb_timestamptz_type(&column.data_type) {
        return None;
    }
    Some((|| {
        let value = parse_timestamptz_value(text)?;
        let Value::TimestampTz(timestamp) = value else {
            return Err(crate::tr!("Invalid timestamp with time zone."));
        };
        if timestamp.timestamp_subsec_nanos() % 1_000 != 0 {
            return Err(crate::tr!("DuckDB TIMESTAMPTZ supports microsecond precision only"));
        }
        Ok(Value::TimestampTz(timestamp))
    })())
}

fn is_postgres_numeric_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut cursor = usize::from(bytes.first().is_some_and(|byte| matches!(*byte, b'+' | b'-')));
    let integer_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let has_integer = cursor > integer_start;
    let mut has_fraction = false;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let fraction_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        has_fraction = cursor > fraction_start;
    }
    if !has_integer && !has_fraction {
        return false;
    }
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let exponent_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == exponent_start {
            return false;
        }
    }
    cursor == bytes.len()
}

fn mysql_bit_width(data_type: &str) -> Option<u32> {
    data_type
        .strip_prefix("bit(")?
        .strip_suffix(')')?
        .parse::<u32>()
        .ok()
        .filter(|width| (1..=64).contains(width))
}

fn parse_mysql_bit_value(text: &str, width: u32) -> Result<Value, String> {
    if width == 1 {
        return parse_bool_value(text);
    }
    let number = text.parse::<u64>().map_err(|_| crate::tr!("Invalid integer"))?;
    let maximum = if width == 64 {
        i64::MAX as u64
    } else {
        (1u64 << width) - 1
    };
    if number > maximum {
        return Err(crate::tr!("Integer is outside the supported BIT range"));
    }
    Ok(Value::Int(number as i64))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TypeKind {
    Bool,
    Int,
    Float,
    Decimal,
    Uuid,
    Json,
    TimestampTz,
    DateTime,
    Date,
    Time,
    Text,
}

/// Map a lowercased `data_type` string to a coarse `TypeKind`. Order
/// of checks matters because several SQL types share substrings — for
/// example `timestamptz` / `timestamp with time zone` must be matched
/// before bare `timestamp`, and `tinyint(1)` (MySQL bool) must be
/// matched before generic `tinyint` / `int` patterns.
pub(super) fn classify_type(dt: &str) -> TypeKind {
    if let Some(kind) = classify_bool_or_bit(dt) {
        return kind;
    }
    if dt.contains("uuid") {
        return TypeKind::Uuid;
    }
    if is_json_like(dt) {
        return TypeKind::Json;
    }
    if let Some(kind) = classify_temporal(dt) {
        return kind;
    }
    classify_numeric(dt).unwrap_or(TypeKind::Text)
}

// Postgres `format_type()` returns "bit(1)" for length-1 BIT columns
// (not the bare "bit" the original guard expected). Both forms classify
// as Bool so the cell renders as a checkbox rather than a text editor
// that rejects "true"/"false" with "Invalid integer". Wider BIT(n)
// values are integers.
fn classify_bool_or_bit(dt: &str) -> Option<TypeKind> {
    if matches!(dt, "bool" | "boolean" | "bit" | "bit(1)" | "tinyint(1)") {
        return Some(TypeKind::Bool);
    }
    if dt.starts_with("bit(") {
        return Some(TypeKind::Int);
    }
    None
}

fn is_json_like(dt: &str) -> bool {
    dt.contains("json")
        || matches!(
            dt,
            "object"
                | "array"
                | "objectid"
                | "bsontimestamp"
                | "regex"
                | "javascript"
                | "javascriptwithscope"
                | "symbol"
                | "undefined"
                | "dbpointer"
                | "minkey"
                | "maxkey"
        )
}

fn classify_temporal(dt: &str) -> Option<TypeKind> {
    if dt.contains("timestamptz") || dt.contains("with time zone") {
        return Some(TypeKind::TimestampTz);
    }
    if dt.contains("timestamp") || dt.contains("datetime") {
        return Some(TypeKind::DateTime);
    }
    if dt == "date" || (dt.starts_with("date") && !dt.contains("datetime") && !dt.contains("time")) {
        return Some(TypeKind::Date);
    }
    if dt == "time" || dt.starts_with("time(") || dt == "time without time zone" {
        return Some(TypeKind::Time);
    }
    None
}

fn classify_numeric(dt: &str) -> Option<TypeKind> {
    if matches!(dt, "decimal" | "numeric" | "money") || dt.starts_with("decimal(") || dt.starts_with("numeric(") {
        return Some(TypeKind::Decimal);
    }
    if matches!(dt, "float" | "double" | "real" | "double precision") || dt.starts_with("float(") {
        return Some(TypeKind::Float);
    }
    if is_integer_type(dt) {
        return Some(TypeKind::Int);
    }
    None
}

fn is_integer_type(dt: &str) -> bool {
    matches!(
        dt,
        "int"
            | "int2"
            | "int4"
            | "int8"
            | "integer"
            | "smallint"
            | "bigint"
            | "tinyint"
            | "mediumint"
            | "serial"
            | "bigserial"
            | "smallserial"
    ) || dt.starts_with("int(")
        || dt.starts_with("integer(")
        || dt.starts_with("smallint(")
        || dt.starts_with("bigint(")
        || dt.starts_with("tinyint(")
        || dt.starts_with("mediumint(")
}

pub(super) fn parse_bool_value(text: &str) -> Result<Value, String> {
    match text.to_ascii_lowercase().as_str() {
        "true" | "t" | "1" | "yes" | "y" | "on" => Ok(Value::Bool(true)),
        "false" | "f" | "0" | "no" | "n" | "off" => Ok(Value::Bool(false)),
        _ => Err(crate::tr!("Invalid boolean. Use true/false, yes/no, or 1/0.")),
    }
}

pub(super) fn parse_int_value(text: &str) -> Result<Value, String> {
    text.parse::<i64>()
        .map(Value::Int)
        .map_err(|_| crate::tr!("Invalid integer"))
}

pub(super) fn parse_float_value(text: &str) -> Result<Value, String> {
    tablepro_core::parse_float_input(text)
        .map(Value::Float)
        .map_err(|_| crate::tr!("Invalid number"))
}

pub(super) fn parse_decimal_value(text: &str) -> Result<Value, String> {
    rust_decimal::Decimal::from_str_exact(text)
        .map(Value::Decimal)
        .map_err(|_| crate::tr!("Invalid decimal"))
}

pub(super) fn parse_uuid_value(text: &str) -> Result<Value, String> {
    uuid::Uuid::parse_str(text)
        .map(Value::Uuid)
        .map_err(|_| crate::tr!("Invalid UUID. Expected 8-4-4-4-12 hex digits."))
}

pub(super) fn parse_json_value(text: &str) -> Result<Value, String> {
    serde_json::from_str::<serde_json::Value>(text)
        .map(Value::Json)
        .map_err(|e| crate::tr!("Invalid JSON: {error}").replace("{error}", &e.to_string()))
}

pub(super) fn parse_timestamptz_value(text: &str) -> Result<Value, String> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(text) {
        return Ok(Value::TimestampTz(dt.with_timezone(&chrono::Utc)));
    }
    Err(crate::tr!(
        "Invalid timestamp. Use ISO 8601, e.g. 2024-01-15T14:30:00Z."
    ))
}

pub(super) fn parse_datetime_value(text: &str) -> Result<Value, String> {
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ];
    for fmt in &formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(text, fmt) {
            return Ok(Value::DateTime(dt));
        }
    }
    Err(crate::tr!("Invalid datetime. Use YYYY-MM-DD HH:MM:SS."))
}

pub(super) fn parse_date_value(text: &str) -> Result<Value, String> {
    chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map(Value::Date)
        .map_err(|_| crate::tr!("Invalid date. Use YYYY-MM-DD."))
}

pub(super) fn parse_time_value(text: &str) -> Result<Value, String> {
    let formats = ["%H:%M:%S", "%H:%M:%S%.f", "%H:%M"];
    for fmt in &formats {
        if let Ok(t) = chrono::NaiveTime::parse_from_str(text, fmt) {
            return Ok(Value::Time(t));
        }
    }
    Err(crate::tr!("Invalid time. Use HH:MM:SS."))
}

#[cfg(test)]
#[path = "../../../../core/tests/support/parser_contract.rs"]
mod parser_contract;

#[cfg(test)]
#[path = "../../../tests/support/value_parse.rs"]
mod tests;
