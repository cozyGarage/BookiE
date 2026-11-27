use std::collections::BTreeMap;
use std::str::FromStr;

use bookie_core::Value;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime};

use crate::rules::{self, Carrier, Rule};

pub type Cells = Result<BTreeMap<i64, Option<String>>, String>;

pub fn value_from_text(carrier: Carrier, text: &str) -> Option<Value> {
    Some(match carrier {
        Carrier::Text => Value::Text(text.to_string()),
        Carrier::JsonText => Value::JsonText(text.to_string()),
        Carrier::Int => Value::Int(text.parse().ok()?),
        Carrier::Float => Value::Float(text.parse().ok()?),
        Carrier::Decimal => Value::Decimal(rust_decimal::Decimal::from_str_exact(text).ok()?),
        Carrier::Bool => Value::Bool(text.parse().ok()?),
        Carrier::Bytes => Value::Bytes(match text.strip_prefix("\\x") {
            Some(hex) => hex::decode(hex).ok()?,
            None => text.as_bytes().to_vec(),
        }),
        Carrier::Date => Value::Date(NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?),
        Carrier::Time => Value::Time(NaiveTime::parse_from_str(text, "%H:%M:%S%.f").ok()?),
        Carrier::DateTime => Value::DateTime(
            NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f")
                .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f"))
                .ok()?,
        ),
        Carrier::TimestampTz => Value::TimestampTz(DateTime::parse_from_rfc3339(text).ok()?.to_utc()),
        Carrier::Uuid => Value::Uuid(uuid::Uuid::from_str(text).ok()?),
        Carrier::Json => Value::Json(Box::new(serde_json::from_str(text).ok()?)),
        Carrier::Undecodable => {
            if !text.starts_with("<undecodable") {
                return None;
            }
            Value::Undecodable(text.to_string())
        }
    })
}

pub fn sqlite_any_payload(text: &str) -> Option<String> {
    let tagged = text.strip_prefix("bookie:sqlite-any:v1:")?;
    let (kind, payload) = tagged.split_once(':')?;
    Some(match kind {
        "blob" => format!("\\x{payload}"),
        _ => payload.to_string(),
    })
}

pub fn check_text(
    rule: Rule,
    carrier: Carrier,
    text: Option<&str>,
    server: Option<&str>,
    server_text: Option<&str>,
) -> Result<(), String> {
    let Some(text) = text else {
        return rules::check(rule, carrier, &Value::Null, server);
    };
    match value_from_text(carrier, text) {
        Some(value) => rules::check(rule, carrier, &value, server),
        None if server_text
            .is_some_and(|server_text| rules::trim_fraction(server_text) == rules::trim_fraction(text)) =>
        {
            Ok(())
        }
        None => Err(format!(
            "artefact text {text:?} is not a {carrier:?}; server text is {server_text:?}"
        )),
    }
}

pub fn delimited_cells(text: &str, delimiter: u8, has_header: bool, null_marker: Option<&str>) -> Cells {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(has_header)
        .flexible(false)
        .from_reader(text.as_bytes());
    let mut cells = BTreeMap::new();
    for record in reader.records() {
        let record = record.map_err(|error| format!("artefact is not valid delimited text: {error}"))?;
        let id = record
            .get(0)
            .and_then(|id| id.parse().ok())
            .ok_or_else(|| format!("artefact row without an id: {record:?}"))?;
        let value = record.get(1).ok_or("artefact row without a value")?;
        let value = match null_marker {
            Some(marker) if value == marker => None,
            _ => Some(value.to_string()),
        };
        cells.insert(id, value);
    }
    Ok(cells)
}

pub type RawRow = BTreeMap<String, Box<serde_json::value::RawValue>>;

pub fn json_cells(rows: &[RawRow], keep_raw: bool) -> Cells {
    let mut cells = BTreeMap::new();
    for row in rows {
        let id = row
            .get("id")
            .and_then(|id| id.get().parse().ok())
            .ok_or_else(|| format!("artefact object without an integer id: {row:?}"))?;
        let value = row
            .get("v")
            .ok_or_else(|| format!("artefact object without v: {row:?}"))?;
        cells.insert(id, json_text(value.get(), keep_raw)?);
    }
    Ok(cells)
}

fn json_text(raw: &str, keep_raw: bool) -> Result<Option<String>, String> {
    if raw == "null" {
        return Ok(None);
    }
    if raw.starts_with('"') && !keep_raw {
        return serde_json::from_str::<String>(raw)
            .map(Some)
            .map_err(|error| format!("artefact string {raw:?}: {error}"));
    }
    Ok(Some(raw.to_string()))
}

#[cfg(test)]
#[path = "artefact_tests.rs"]
mod tests;
