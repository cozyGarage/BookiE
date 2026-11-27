use bookie_core::Value;
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Carrier {
    Bool,
    Int,
    Float,
    Text,
    Bytes,
    Date,
    Time,
    DateTime,
    TimestampTz,
    Decimal,
    Uuid,
    Json,
    JsonText,
    Undecodable,
}

impl Carrier {
    pub fn of(value: &Value) -> Option<Carrier> {
        Some(match value {
            Value::Null => return None,
            Value::Bool(_) => Carrier::Bool,
            Value::Int(_) => Carrier::Int,
            Value::Float(_) => Carrier::Float,
            Value::Text(_) => Carrier::Text,
            Value::Bytes(_) => Carrier::Bytes,
            Value::Date(_) => Carrier::Date,
            Value::Time(_) => Carrier::Time,
            Value::DateTime(_) => Carrier::DateTime,
            Value::TimestampTz(_) => Carrier::TimestampTz,
            Value::Decimal(_) => Carrier::Decimal,
            Value::Uuid(_) => Carrier::Uuid,
            Value::Json(_) => Carrier::Json,
            Value::JsonText(_) => Carrier::JsonText,
            Value::Undecodable(_) => Carrier::Undecodable,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    Exact,
    ServerCast,
    Number,
    Float,
    Float32,
    Bytes,
    Bool,
    Temporal,
    Instant,
    Json,
    Refusal,
}

impl Rule {
    pub fn referee_column(self) -> RefereeColumn {
        match self {
            Rule::Bytes => RefereeColumn::Hex,
            Rule::Instant => RefereeColumn::Epoch,
            _ => RefereeColumn::Text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefereeColumn {
    Text,
    Hex,
    Epoch,
}

pub fn check(rule: Rule, carrier: Carrier, value: &Value, referee: Option<&str>) -> Result<(), String> {
    let Some(server) = referee else {
        return match value {
            Value::Null => Ok(()),
            other => Err(format!("server has NULL, BookiE has {}", describe(other))),
        };
    };
    if matches!(value, Value::Null) {
        return Err(format!("server has {server:?}, BookiE has NULL"));
    }
    if rule == Rule::Refusal {
        return match value {
            Value::Undecodable(_) => Ok(()),
            other => Err(format!("expected an explicit refusal, BookiE has {}", describe(other))),
        };
    }
    let found = Carrier::of(value);
    if found != Some(carrier) {
        return Err(format!(
            "expected carrier {carrier:?}, BookiE has {}; server has {server:?}",
            describe(value)
        ));
    }
    compare(rule, value, server)
}

fn compare(rule: Rule, value: &Value, server: &str) -> Result<(), String> {
    let (ours, theirs) = match rule {
        Rule::Float => return float_equal(value, server, false),
        Rule::Float32 => return float_equal(value, server, true),
        Rule::Bool => return bool_equal(value, server),
        Rule::Exact | Rule::ServerCast => (exact_text(value), server.to_string()),
        Rule::Number => (number_text(value), canonical_decimal(server)),
        Rule::Bytes => (bytes_hex(value), server.to_ascii_lowercase()),
        Rule::Temporal => (
            exact_text(value).map(|text| trim_fraction(&text)),
            trim_fraction(server),
        ),
        Rule::Instant => (instant_epoch(value), canonical_decimal(server)),
        Rule::Json => (json_compact(value), reparse_json(server)),
        Rule::Refusal => (None, String::new()),
    };
    let Some(ours) = ours else {
        return Err(format!("rule {rule:?} cannot compare {}", describe(value)));
    };
    if ours == theirs {
        Ok(())
    } else {
        Err(format!("server {theirs:?}, BookiE {ours:?} ({})", describe(value)))
    }
}

fn exact_text(value: &Value) -> Option<String> {
    let text = match value {
        Value::Bool(flag) => flag.to_string(),
        Value::Int(number) => number.to_string(),
        Value::Text(text) | Value::JsonText(text) => text.clone(),
        Value::Decimal(number) => number.to_string(),
        Value::Uuid(id) => id.hyphenated().to_string(),
        Value::Date(day) => day.format("%Y-%m-%d").to_string(),
        Value::Time(time) => time.format("%H:%M:%S%.f").to_string(),
        Value::DateTime(stamp) => stamp.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
        _ => return None,
    };
    Some(text)
}

fn bytes_hex(value: &Value) -> Option<String> {
    match value {
        Value::Bytes(bytes) => Some(hex::encode(bytes)),
        _ => None,
    }
}

fn float_equal(value: &Value, server: &str, single: bool) -> Result<(), String> {
    let Value::Float(ours) = value else {
        return Err(format!("rule float cannot compare {}", describe(value)));
    };
    let theirs: f64 = server
        .trim()
        .parse()
        .map_err(|_| format!("server float {server:?} is not a number"))?;
    let same = if single {
        (*ours as f32).to_bits() == (theirs as f32).to_bits()
    } else {
        ours.to_bits() == theirs.to_bits()
    };
    if (ours.is_nan() && theirs.is_nan()) || same {
        Ok(())
    } else {
        Err(format!("server {server:?} = {theirs:e}, BookiE {ours:e}"))
    }
}

fn bool_equal(value: &Value, server: &str) -> Result<(), String> {
    let theirs = match server {
        "t" | "true" | "1" => true,
        "f" | "false" | "0" => false,
        other => return Err(format!("server boolean {other:?} is not t/f/true/false/1/0")),
    };
    match value {
        Value::Bool(ours) if *ours == theirs => Ok(()),
        other => Err(format!("server {server:?}, BookiE {}", describe(other))),
    }
}

pub fn trim_fraction(text: &str) -> String {
    let Some(dot) = text.rfind('.') else {
        return text.to_string();
    };
    let digits = text[dot + 1..].chars().take_while(char::is_ascii_digit).count();
    let end = dot + 1 + digits;
    let fraction = text[dot + 1..end].trim_end_matches('0');
    let head = &text[..dot];
    let tail = &text[end..];
    if fraction.is_empty() {
        format!("{head}{tail}")
    } else {
        format!("{head}.{fraction}{tail}")
    }
}

pub fn canonical_decimal(text: &str) -> String {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let whole = whole.trim_start_matches('0');
    let fraction = fraction.trim_end_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let body = if fraction.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{fraction}")
    };
    if negative && body != "0" {
        format!("-{body}")
    } else {
        body
    }
}

fn instant_epoch(value: &Value) -> Option<String> {
    let stamp = match value {
        Value::TimestampTz(stamp) => *stamp,
        Value::DateTime(stamp) => stamp.and_utc(),
        Value::Date(day) => day.and_time(chrono::NaiveTime::MIN).and_utc(),
        _ => return None,
    };
    Some(epoch_text(&stamp))
}

fn number_text(value: &Value) -> Option<String> {
    match value {
        Value::Int(_) | Value::Decimal(_) | Value::Text(_) => exact_text(value).map(|text| canonical_decimal(&text)),
        _ => None,
    }
}

pub fn epoch_text(stamp: &DateTime<Utc>) -> String {
    let nanos = i128::from(stamp.timestamp()) * 1_000_000_000 + i128::from(stamp.timestamp_subsec_nanos());
    let sign = if nanos < 0 { "-" } else { "" };
    let magnitude = nanos.unsigned_abs();
    canonical_decimal(&format!(
        "{sign}{}.{:09}",
        magnitude / 1_000_000_000,
        magnitude % 1_000_000_000
    ))
}

fn json_compact(value: &Value) -> Option<String> {
    match value {
        Value::Json(json) => Some(json.to_string()),
        Value::JsonText(text) => Some(reparse_json(text)),
        _ => None,
    }
}

fn reparse_json(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .map_or_else(|_| format!("<invalid JSON {text:?}>"), |json| json.to_string())
}

pub fn describe(value: &Value) -> String {
    let text = format!("{value:?}");
    if text.chars().count() > 160 {
        let prefix: String = text.chars().take(160).collect();
        format!("{prefix}...")
    } else {
        text
    }
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
