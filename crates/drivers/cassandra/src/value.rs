use bookie_core::Value;
use chrono::{DateTime, NaiveDate, NaiveTime};
use scylla::value::CqlValue;
use serde_json::json;

const DAYS_FROM_CE_TO_EPOCH: i64 = 719_163;
const CQL_DATE_EPOCH: i64 = 1 << 31;
const EXACT_JSON_INTEGER: i64 = 1 << 53;

pub(crate) fn cell(value: Option<CqlValue>) -> Value {
    let Some(value) = value else { return Value::Null };
    match value {
        CqlValue::Empty => Value::Null,
        CqlValue::Ascii(text) | CqlValue::Text(text) => Value::Text(text),
        CqlValue::Boolean(flag) => Value::Bool(flag),
        CqlValue::TinyInt(number) => Value::Int(i64::from(number)),
        CqlValue::SmallInt(number) => Value::Int(i64::from(number)),
        CqlValue::Int(number) => Value::Int(i64::from(number)),
        CqlValue::BigInt(number) => Value::Int(number),
        CqlValue::Counter(counter) => Value::Int(counter.0),
        CqlValue::Float(number) => Value::Float(f64::from(number)),
        CqlValue::Double(number) => Value::Float(number),
        CqlValue::Varint(varint) => exact_number(&decimal_digits(varint.as_signed_bytes_be_slice(), 0)),
        CqlValue::Decimal(decimal) => {
            let (bytes, scale) = decimal.as_signed_be_bytes_slice_and_exponent();
            exact_number(&decimal_digits(bytes, scale))
        }
        CqlValue::Blob(bytes) => Value::Bytes(bytes),
        CqlValue::Uuid(uuid) => Value::Uuid(uuid),
        CqlValue::Timeuuid(uuid) => Value::Uuid(uuid.into()),
        CqlValue::Date(date) => {
            let days = i64::from(date.0) - CQL_DATE_EPOCH + DAYS_FROM_CE_TO_EPOCH;
            i32::try_from(days)
                .ok()
                .and_then(NaiveDate::from_num_days_from_ce_opt)
                .map_or_else(|| Value::Undecodable("date".into()), Value::Date)
        }
        CqlValue::Time(time) => {
            let seconds = time.0.div_euclid(1_000_000_000);
            let nanos = time.0.rem_euclid(1_000_000_000);
            u32::try_from(seconds)
                .ok()
                .zip(u32::try_from(nanos).ok())
                .and_then(|(seconds, nanos)| NaiveTime::from_num_seconds_from_midnight_opt(seconds, nanos))
                .map_or_else(|| Value::Undecodable("time".into()), Value::Time)
        }
        CqlValue::Timestamp(timestamp) => DateTime::from_timestamp_millis(timestamp.0)
            .map_or_else(|| Value::Undecodable("timestamp".into()), Value::TimestampTz),
        CqlValue::Inet(address) => Value::Text(address.to_string()),
        CqlValue::Duration(duration) => Value::Text(format!(
            "{}mo{}d{}ns",
            duration.months, duration.days, duration.nanoseconds
        )),
        nested => Value::JsonText(to_json(&nested).to_string()),
    }
}

fn exact_number(digits: &str) -> Value {
    digits
        .parse::<rust_decimal::Decimal>()
        .map_or_else(|_| Value::Text(digits.to_string()), Value::Decimal)
}

fn to_json(value: &CqlValue) -> serde_json::Value {
    match value {
        CqlValue::Empty => serde_json::Value::Null,
        CqlValue::Ascii(text) | CqlValue::Text(text) => json!(text),
        CqlValue::Boolean(flag) => json!(flag),
        CqlValue::TinyInt(number) => json!(number),
        CqlValue::SmallInt(number) => json!(number),
        CqlValue::Int(number) => json!(number),
        CqlValue::BigInt(number) => big_integer(*number),
        CqlValue::Counter(counter) => big_integer(counter.0),
        CqlValue::Float(number) => json!(number),
        CqlValue::Double(number) => json!(number),
        CqlValue::List(items) | CqlValue::Set(items) | CqlValue::Vector(items) => {
            serde_json::Value::Array(items.iter().map(to_json).collect())
        }
        CqlValue::Tuple(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| item.as_ref().map_or(serde_json::Value::Null, to_json))
                .collect(),
        ),
        CqlValue::Map(pairs) => serde_json::Value::Array(
            pairs
                .iter()
                .map(|(key, value)| serde_json::Value::Array(vec![to_json(key), to_json(value)]))
                .collect(),
        ),
        CqlValue::UserDefinedType { fields, .. } => serde_json::Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), value.as_ref().map_or(serde_json::Value::Null, to_json)))
                .collect(),
        ),
        other => match cell(Some(other.clone())) {
            Value::Text(text) => json!(text),
            Value::Decimal(decimal) => json!(decimal.to_string()),
            Value::Uuid(uuid) => json!(uuid.to_string()),
            Value::Date(date) => json!(date.to_string()),
            Value::Time(time) => json!(time.to_string()),
            Value::TimestampTz(timestamp) => json!(timestamp.to_rfc3339()),
            Value::Bytes(bytes) => json!(bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()),
            _ => serde_json::Value::Null,
        },
    }
}

fn big_integer(number: i64) -> serde_json::Value {
    if number.abs() > EXACT_JSON_INTEGER {
        json!(number.to_string())
    } else {
        json!(number)
    }
}

fn decimal_digits(bytes: &[u8], scale: i32) -> String {
    let negative = bytes.first().is_some_and(|byte| byte & 0x80 != 0);
    let mut magnitude: Vec<u8> = if negative {
        let mut inverted: Vec<u8> = bytes.iter().map(|byte| !byte).collect();
        for byte in inverted.iter_mut().rev() {
            let (next, carry) = byte.overflowing_add(1);
            *byte = next;
            if !carry {
                break;
            }
        }
        inverted
    } else {
        bytes.to_vec()
    };
    let mut digits = Vec::new();
    while magnitude.iter().any(|byte| *byte != 0) {
        let mut remainder = 0u32;
        for byte in magnitude.iter_mut() {
            let current = remainder * 256 + u32::from(*byte);
            *byte = u8::try_from(current / 10).unwrap_or_default();
            remainder = current % 10;
        }
        digits.push(char::from(b'0' + u8::try_from(remainder).unwrap_or_default()));
    }
    if digits.is_empty() {
        digits.push('0');
    }
    digits.reverse();
    let mut text: String = digits.into_iter().collect();
    if scale > 0 {
        let scale = usize::try_from(scale).unwrap_or_default();
        if text.len() <= scale {
            text = format!("{}{text}", "0".repeat(scale - text.len() + 1));
        }
        text.insert(text.len() - scale, '.');
    } else if scale < 0 && text != "0" {
        text.push_str(&"0".repeat(usize::try_from(-scale).unwrap_or_default()));
    }
    if negative { format!("-{text}") } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scylla::value::{Counter, CqlDate, CqlDecimal, CqlTime, CqlTimestamp, CqlVarint};

    #[test]
    fn integers_keep_their_width_and_counters_are_integers() {
        assert_eq!(
            cell(Some(CqlValue::BigInt(9_007_199_254_740_993))),
            Value::Int(9_007_199_254_740_993)
        );
        assert_eq!(cell(Some(CqlValue::TinyInt(-5))), Value::Int(-5));
        assert_eq!(cell(Some(CqlValue::Counter(Counter(7)))), Value::Int(7));
        assert_eq!(cell(None), Value::Null);
    }

    #[test]
    fn varints_and_decimals_keep_every_digit() {
        let huge = CqlVarint::from_signed_bytes_be(vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(cell(Some(CqlValue::Varint(huge))), exact("18446744073709551616"));
        let negative = CqlVarint::from_signed_bytes_be(vec![0xff, 0x38]);
        assert_eq!(cell(Some(CqlValue::Varint(negative))), exact("-200"));
        let money = CqlDecimal::from_signed_be_bytes_and_exponent(vec![0x30, 0x39], 2);
        assert_eq!(cell(Some(CqlValue::Decimal(money))), exact("123.45"));
        let small = CqlDecimal::from_signed_be_bytes_and_exponent(vec![0x05], 3);
        assert_eq!(cell(Some(CqlValue::Decimal(small))), exact("0.005"));
        let scaled_up = CqlDecimal::from_signed_be_bytes_and_exponent(vec![0x07], -2);
        assert_eq!(cell(Some(CqlValue::Decimal(scaled_up))), exact("700"));
    }

    fn exact(text: &str) -> Value {
        match text.parse::<rust_decimal::Decimal>() {
            Ok(decimal) => Value::Decimal(decimal),
            Err(_) => Value::Text(text.into()),
        }
    }

    #[test]
    fn dates_times_and_timestamps_become_typed_values() {
        let epoch = CqlDate(1 << 31);
        assert_eq!(
            cell(Some(CqlValue::Date(epoch))),
            Value::Date(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
        );
        let day_after = CqlDate((1 << 31) + 1);
        assert_eq!(
            cell(Some(CqlValue::Date(day_after))),
            Value::Date(NaiveDate::from_ymd_opt(1970, 1, 2).unwrap())
        );
        assert_eq!(
            cell(Some(CqlValue::Time(CqlTime(3_723_000_000_500)))),
            Value::Time(NaiveTime::from_hms_nano_opt(1, 2, 3, 500).unwrap())
        );
        assert_eq!(
            cell(Some(CqlValue::Timestamp(CqlTimestamp(1_000)))),
            Value::TimestampTz(DateTime::from_timestamp_millis(1_000).unwrap())
        );
        assert_eq!(
            cell(Some(CqlValue::Time(CqlTime(-1)))),
            Value::Undecodable("time".into())
        );
    }

    #[test]
    fn collections_become_json_with_wide_integers_kept_as_text() {
        let list = CqlValue::List(vec![CqlValue::Int(1), CqlValue::BigInt(9_007_199_254_740_993)]);
        assert_eq!(cell(Some(list)), Value::JsonText(r#"[1,"9007199254740993"]"#.into()));
        let map = CqlValue::Map(vec![(CqlValue::Text("a".into()), CqlValue::Boolean(true))]);
        assert_eq!(cell(Some(map)), Value::JsonText(r#"[["a",true]]"#.into()));
        let udt = CqlValue::UserDefinedType {
            keyspace: "ks".into(),
            name: "point".into(),
            fields: vec![("x".into(), Some(CqlValue::Int(1))), ("y".into(), None)],
        };
        assert_eq!(cell(Some(udt)), Value::JsonText(r#"{"x":1,"y":null}"#.into()));
    }
}
