use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::model::Cell;

pub const EDITABLE_COLUMNS: std::ops::Range<usize> = 1..4;

pub fn is_editable(column: usize, cell: &Cell) -> bool {
    EDITABLE_COLUMNS.contains(&column) && !matches!(cell, Cell::Undecodable(_))
}

fn parse_bool(text: &str) -> Result<Cell, &'static str> {
    match text.to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "1" => Ok(Cell::Bool(true)),
        "false" | "f" | "no" | "n" | "0" => Ok(Cell::Bool(false)),
        _ => Err("not true or false"),
    }
}

fn parse_time(text: &str) -> Result<Cell, &'static str> {
    ["%H:%M:%S%.f", "%H:%M"]
        .iter()
        .find_map(|format| NaiveTime::parse_from_str(text, format).ok())
        .map(|time| Cell::Time(time.to_string()))
        .ok_or("not a time")
}

fn parse_naive(text: &str) -> Option<NaiveDateTime> {
    [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
    .or_else(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0))
}

fn parse_bytes(text: &str) -> Result<Cell, &'static str> {
    let digits = ["0x", "0X", "\\x"]
        .iter()
        .find_map(|prefix| text.strip_prefix(prefix))
        .unwrap_or(text);
    if !digits.len().is_multiple_of(2) || !digits.is_ascii() {
        return Err("not hexadecimal bytes");
    }
    let bytes: Result<Vec<u8>, _> = digits
        .as_bytes()
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("zz"), 16))
        .collect();
    bytes.map(Cell::Bytes).map_err(|_| "not hexadecimal bytes")
}

pub fn parse_edit(kind: &str, text: &str) -> Result<Cell, &'static str> {
    let trimmed = text.trim();
    match kind {
        "boolean" => parse_bool(trimmed),
        "integer" => trimmed.parse().map(Cell::Int).map_err(|_| "not a whole number"),
        "float" => trimmed
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(Cell::Float)
            .ok_or("not a number"),
        "decimal" => Decimal::from_str_exact(trimmed)
            .map(|value| Cell::Decimal(value.to_string()))
            .map_err(|_| "not a number"),
        "date" => NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
            .map(|date| Cell::Date(date.to_string()))
            .map_err(|_| "not a date"),
        "time" => parse_time(trimmed),
        "datetime" => parse_naive(trimmed)
            .map(|value| Cell::DateTime(value.to_string()))
            .ok_or("not a timestamp"),
        "timestamptz" => DateTime::parse_from_rfc3339(trimmed)
            .map(|value| value.with_timezone(&Utc))
            .ok()
            .or_else(|| parse_naive(trimmed).map(|naive| naive.and_utc()))
            .map(|value| Cell::TimestampTz(value.to_rfc3339()))
            .ok_or("not a timestamp"),
        "uuid" => Uuid::parse_str(trimmed)
            .map(|value| Cell::Uuid(value.to_string()))
            .map_err(|_| "not a UUID"),
        "json" => serde_json::from_str::<serde_json::Value>(trimmed)
            .map(|value| Cell::Json(value.to_string()))
            .map_err(|_| "not JSON"),
        "bytes" => parse_bytes(trimmed),
        "json_text" => Ok(Cell::JsonText(text.to_string())),
        "text" | "null" => Ok(Cell::Text(text.to_string())),
        _ => Err("this cell cannot be edited"),
    }
}
