use bookie_core::{DriverError, Value};
use chrono::NaiveDate;
use oracledb::{OracleNumber, OracleTimestamp, Row};
use rust_decimal::Decimal;
use std::str::FromStr;

pub(crate) fn decimal_or_undecodable(digits: &str, type_name: &str) -> Value {
    match Decimal::from_str(digits) {
        Ok(value) => Value::Decimal(value),
        Err(_) => Value::Undecodable(type_name.to_string()),
    }
}

pub(crate) fn timestamp_value(stamp: &OracleTimestamp) -> Option<Value> {
    let date = NaiveDate::from_ymd_opt(
        i32::from(stamp.year()),
        u32::from(stamp.month()),
        u32::from(stamp.day()),
    )?;
    date.and_hms_nano_opt(
        u32::from(stamp.hour()),
        u32::from(stamp.minute()),
        u32::from(stamp.second()),
        stamp.nanoseconds(),
    )
    .map(Value::DateTime)
}

pub(crate) fn cell(row: &Row, index: usize, db_type: &str, type_name: &str) -> Result<Value, DriverError> {
    let undecodable = || Value::Undecodable(type_name.to_string());
    let read = |error: oracledb::Error| DriverError::Internal(format!("oracle column {index}: {error}"));
    Ok(match db_type {
        "DB_TYPE_NUMBER" | "DB_TYPE_BINARY_INTEGER" => row
            .get::<Option<OracleNumber>>(index)
            .map_err(read)?
            .map_or(Value::Null, |number| {
                decimal_or_undecodable(&number.to_string(), type_name)
            }),
        "DB_TYPE_VARCHAR"
        | "DB_TYPE_CHAR"
        | "DB_TYPE_NVARCHAR"
        | "DB_TYPE_NCHAR"
        | "DB_TYPE_LONG"
        | "DB_TYPE_LONG_NVARCHAR"
        | "DB_TYPE_CLOB"
        | "DB_TYPE_NCLOB"
        | "DB_TYPE_ROWID"
        | "DB_TYPE_UROWID" => row
            .get::<Option<String>>(index)
            .map_err(read)?
            .map_or(Value::Null, Value::Text),
        "DB_TYPE_RAW" | "DB_TYPE_LONG_RAW" | "DB_TYPE_BLOB" => row
            .get::<Option<Vec<u8>>>(index)
            .map_err(read)?
            .map_or(Value::Null, Value::Bytes),
        "DB_TYPE_BINARY_FLOAT" | "DB_TYPE_BINARY_DOUBLE" => row
            .get::<Option<f64>>(index)
            .map_err(read)?
            .map_or(Value::Null, Value::Float),
        "DB_TYPE_BOOLEAN" => row
            .get::<Option<bool>>(index)
            .map_err(read)?
            .map_or(Value::Null, Value::Bool),
        "DB_TYPE_DATE" | "DB_TYPE_TIMESTAMP" => match row.get::<Option<OracleTimestamp>>(index).map_err(read)? {
            None => Value::Null,
            Some(stamp) => timestamp_value(&stamp).unwrap_or_else(undecodable),
        },
        _ => {
            if row.get::<Option<String>>(index).is_ok_and(|value| value.is_none()) {
                Value::Null
            } else {
                undecodable()
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_that_fits_keeps_every_digit_and_its_scale() {
        assert_eq!(
            decimal_or_undecodable("12345678901234567890.12345678", "NUMBER(38,10)"),
            Value::Decimal(Decimal::from_str("12345678901234567890.12345678").unwrap_or_default())
        );
        assert_eq!(
            decimal_or_undecodable("0.10", "NUMBER"),
            Value::Decimal(Decimal::new(10, 2))
        );
    }

    #[test]
    fn a_number_wider_than_a_decimal_is_marked_undecodable_not_rounded() {
        let digits = "1234567890123456789012345678901234567890";
        assert_eq!(
            decimal_or_undecodable(digits, "NUMBER(40)"),
            Value::Undecodable("NUMBER(40)".into())
        );
    }

    #[test]
    fn a_timestamp_converts_to_a_naive_date_time_with_nanoseconds() {
        let stamp = OracleTimestamp::new_timestamp(2026, 11, 4, 23, 59, 59, 123_456_000);
        let expected = NaiveDate::from_ymd_opt(2026, 11, 4)
            .and_then(|date| date.and_hms_micro_opt(23, 59, 59, 123_456))
            .map(Value::DateTime);
        assert_eq!(timestamp_value(&stamp), expected);
    }
}
