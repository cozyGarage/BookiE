use bookie_core::{QueryResultBudget, Value, single_precision_as_f64};
use sqlx::mysql::{MySql, MySqlColumn, MySqlRow, MySqlValueRef};
use sqlx::{Column, Decode, Row, Type, TypeInfo, ValueRef};

use crate::{packed, temporal};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DecodeKind {
    SignedInteger,
    UnsignedInteger,
    Float,
    Double,
    Decimal,
    Boolean,
    Date,
    Time,
    DateTime,
    Year,
    Bit,
    Geometry,
    EnumSet,
    Timestamp,
    Json,
    Binary,
    Text,
}

#[derive(Clone, Debug)]
pub(crate) struct ColumnDecoder {
    type_name: String,
    kind: DecodeKind,
}

impl ColumnDecoder {
    fn new(type_name: &str) -> Self {
        let type_name = type_name.to_ascii_uppercase();
        let kind = match type_name.as_str() {
            "TINYINT" | "SMALLINT" | "INT" | "MEDIUMINT" | "BIGINT" => DecodeKind::SignedInteger,
            "TINYINT UNSIGNED" | "SMALLINT UNSIGNED" | "INT UNSIGNED" | "MEDIUMINT UNSIGNED" | "BIGINT UNSIGNED" => {
                DecodeKind::UnsignedInteger
            }
            "FLOAT" => DecodeKind::Float,
            "DOUBLE" => DecodeKind::Double,
            "DECIMAL" | "NUMERIC" => DecodeKind::Decimal,
            "BOOLEAN" => DecodeKind::Boolean,
            "DATE" => DecodeKind::Date,
            "TIME" => DecodeKind::Time,
            "DATETIME" => DecodeKind::DateTime,
            "YEAR" => DecodeKind::Year,
            "BIT" => DecodeKind::Bit,
            "GEOMETRY" => DecodeKind::Geometry,
            "ENUM" | "SET" => DecodeKind::EnumSet,
            "TIMESTAMP" => DecodeKind::Timestamp,
            "JSON" => DecodeKind::Json,
            "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "VARBINARY" | "BINARY" => DecodeKind::Binary,
            _ => DecodeKind::Text,
        };
        Self { type_name, kind }
    }
}

pub(crate) fn column_decoders(columns: &[MySqlColumn]) -> Vec<ColumnDecoder> {
    columns
        .iter()
        .map(|column| ColumnDecoder::new(column.type_info().name()))
        .collect()
}

pub(crate) fn decode_row_into(
    row: &MySqlRow,
    decoders: &[ColumnDecoder],
    budget: &mut QueryResultBudget,
    values: &mut Vec<Value>,
) -> bool {
    values.clear();
    if row.len() != decoders.len() {
        return false;
    }
    let mut payload_bytes = 0usize;
    for (index, decoder) in decoders.iter().enumerate() {
        let Ok(raw) = row.try_get_raw(index) else {
            values.clear();
            return false;
        };
        payload_bytes = payload_bytes.saturating_add(mysql_value_payload_bytes(raw.clone()));
        if !budget.can_admit_row_payload(row.len(), payload_bytes) {
            values.clear();
            return false;
        }
        values.push(extract_value(raw, index, decoder));
    }
    if budget.admit(values) {
        true
    } else {
        values.clear();
        false
    }
}

pub(crate) fn extract_row(row: &MySqlRow, decoders: &[ColumnDecoder]) -> Vec<Value> {
    (0..row.len())
        .map(|index| match (row.try_get_raw(index), decoders.get(index)) {
            (Ok(raw), Some(decoder)) => extract_value(raw, index, decoder),
            (_, Some(decoder)) => undecodable(index, &decoder.type_name),
            _ => undecodable(index, "unknown"),
        })
        .collect()
}

fn mysql_value_payload_bytes(raw: MySqlValueRef<'_>) -> usize {
    if raw.is_null() {
        return 0;
    }
    <&[u8] as Decode<MySql>>::decode(raw).map_or(usize::MAX, <[u8]>::len)
}

fn extract_value(raw: MySqlValueRef<'_>, index: usize, decoder: &ColumnDecoder) -> Value {
    if raw.is_null() {
        return zero_calendar(raw, decoder).unwrap_or(Value::Null);
    }
    decode_by_type(raw, decoder).unwrap_or_else(|| undecodable(index, &decoder.type_name))
}

fn decode_by_type(raw: MySqlValueRef<'_>, decoder: &ColumnDecoder) -> Option<Value> {
    match decoder.kind {
        DecodeKind::SignedInteger => decode::<i64>(raw).map(Value::Int),
        DecodeKind::UnsignedInteger => decode::<u64>(raw)
            .map(|value| i64::try_from(value).map_or_else(|_| Value::Text(value.to_string()), Value::Int)),
        DecodeKind::Float => decode::<f32>(raw).map(|value| Value::Float(single_precision_as_f64(value))),
        DecodeKind::Double => decode::<f64>(raw).map(Value::Float),
        DecodeKind::Decimal => decode_decimal_exact(raw),
        DecodeKind::Boolean => decode::<i64>(raw.clone())
            .or_else(|| decode::<u8>(raw).map(i64::from))
            .map(tiny_flag),
        DecodeKind::Date => decode::<chrono::NaiveDate>(raw.clone())
            .map(Value::Date)
            .or_else(|| temporal::calendar_text(raw, false)),
        DecodeKind::Time => temporal::time_value(raw),
        DecodeKind::DateTime => decode::<chrono::NaiveDateTime>(raw.clone())
            .map(Value::DateTime)
            .or_else(|| temporal::calendar_text(raw, true)),
        DecodeKind::Year => temporal::year_value(raw),
        DecodeKind::Bit => packed::bit_value(raw),
        DecodeKind::Geometry => packed::geometry_value(raw),
        DecodeKind::EnumSet => decode::<String>(raw.clone())
            .map(Value::Text)
            .or_else(|| decode::<Vec<u8>>(raw).map(Value::Bytes)),
        DecodeKind::Timestamp => decode::<chrono::DateTime<chrono::Utc>>(raw).map(Value::TimestampTz),
        DecodeKind::Json => decode::<serde_json::Value>(raw).map(json_value),
        DecodeKind::Binary => decode::<Vec<u8>>(raw).map(Value::Bytes),
        DecodeKind::Text => decode::<String>(raw).map(Value::Text),
    }
}

fn zero_calendar(raw: MySqlValueRef<'_>, decoder: &ColumnDecoder) -> Option<Value> {
    match decoder.kind {
        DecodeKind::Date => temporal::calendar_text(raw, false),
        DecodeKind::DateTime | DecodeKind::Timestamp => temporal::calendar_text(raw, true),
        _ => None,
    }
}

fn decode<T>(raw: MySqlValueRef<'_>) -> Option<T>
where
    for<'r> T: Decode<'r, MySql> + Type<MySql>,
{
    let type_info = raw.type_info();
    if !type_info.is_null() && !T::compatible(&type_info) {
        return None;
    }
    <T as Decode<MySql>>::decode(raw).ok()
}

fn undecodable(index: usize, type_name: &str) -> Value {
    tracing::warn!(
        column_index = index + 1,
        type_name,
        "mysql column value could not be decoded; showing it as undecodable"
    );
    Value::Undecodable(type_name.to_string())
}

fn decode_decimal_exact(raw: MySqlValueRef<'_>) -> Option<Value> {
    if raw.is_null() {
        return None;
    }
    <&str as Decode<MySql>>::decode(raw)
        .map(|text| {
            rust_decimal::Decimal::from_str_exact(text)
                .map(Value::Decimal)
                .unwrap_or_else(|_| Value::Text(text.to_owned()))
        })
        .ok()
}

fn tiny_flag(stored: i64) -> Value {
    match stored {
        0 => Value::Bool(false),
        1 => Value::Bool(true),
        other => Value::Int(other),
    }
}

fn json_value(json: serde_json::Value) -> Value {
    Value::Json(Box::new(json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_zero_and_one_read_as_a_flag() {
        assert_eq!(tiny_flag(0), Value::Bool(false));
        assert_eq!(tiny_flag(1), Value::Bool(true));
        assert_eq!(tiny_flag(2), Value::Int(2));
        assert_eq!(tiny_flag(-1), Value::Int(-1));
        assert_eq!(tiny_flag(255), Value::Int(255));
    }

    #[test]
    fn column_decoder_plan_keeps_signed_and_unsigned_integer_classes_distinct() {
        assert_eq!(ColumnDecoder::new("BIGINT").kind, DecodeKind::SignedInteger);
        assert_eq!(ColumnDecoder::new("BIGINT UNSIGNED").kind, DecodeKind::UnsignedInteger);
        assert_eq!(ColumnDecoder::new("DECIMAL").kind, DecodeKind::Decimal);
        assert_eq!(ColumnDecoder::new("mystery").kind, DecodeKind::Text);
    }
}
