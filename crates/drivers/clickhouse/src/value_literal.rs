use bookie_core::{DriverError, Value};
use chrono::Datelike;

pub(super) fn literal(value: &Value) -> Result<String, DriverError> {
    let rendered = match value {
        Value::Null => "NULL".into(),
        Value::Bool(b) => if *b { "true" } else { "false" }.into(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            // ClickHouse spells these out; silently substituting NULL
            // would write a different value than the user typed.
            if f.is_nan() {
                "nan".into()
            } else if f.is_infinite() {
                if f.is_sign_negative() { "-inf" } else { "inf" }.into()
            } else {
                format!("toFloat64({f:e})")
            }
        }
        Value::Text(s) => format!("'{}'", escape_str(s)),
        Value::Bytes(b) => format!("unhex('{}')", hex_encode(b)),
        Value::Date(d) if (1900..=2299).contains(&d.year()) => format!("toDate32('{}')", d.format("%Y-%m-%d")),
        Value::Date(_) => {
            return Err(DriverError::Unsupported(
                "ClickHouse Date32 cannot represent this date".into(),
            ));
        }
        Value::Time(t) => format!("'{}'", t.format("%H:%M:%S%.f")),
        Value::DateTime(dt) => bookie_core::sql_literal::clickhouse_datetime64_literal(*dt, None)
            .ok_or_else(unsupported_datetime64_range)?,
        Value::TimestampTz(ts) => bookie_core::sql_literal::clickhouse_datetime64_literal(ts.naive_utc(), Some("UTC"))
            .ok_or_else(unsupported_datetime64_range)?,
        Value::Decimal(d) => format!("toDecimal128('{d}', {})", d.scale()),
        Value::Uuid(u) => format!("toUUID('{u}')"),
        Value::Json(_) | Value::JsonText(_) => {
            return Err(DriverError::Unsupported(
                "ClickHouse nested values cannot be edited losslessly without their native type metadata".into(),
            ));
        }
        Value::Undecodable(type_name) => {
            return Err(DriverError::Internal(format!(
                "cannot write back an undecodable {type_name} value"
            )));
        }
    };
    Ok(rendered)
}

fn unsupported_datetime64_range() -> DriverError {
    DriverError::Unsupported(
        "ClickHouse DateTime64 cannot represent this timestamp exactly within its 1900-2299 calendar and Int64 precision bounds".into(),
    )
}

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn hex_encode(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(LUT[(byte >> 4) as usize] as char);
        out.push(LUT[(byte & 0xf) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::literal;
    use bookie_core::{DriverError, Value};

    #[test]
    fn date_literals_preserve_date32_boundaries_and_refuse_outside_them() {
        for date in [(1900, 1, 1), (1970, 1, 1), (2149, 6, 6), (2299, 12, 31)] {
            let value = chrono::NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap();
            assert_eq!(literal(&Value::Date(value)).unwrap(), format!("toDate32('{value}')"));
        }
        for date in [(1899, 12, 31), (2300, 1, 1)] {
            let value = chrono::NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap();
            assert!(matches!(literal(&Value::Date(value)), Err(DriverError::Unsupported(_))));
        }
    }

    #[test]
    fn finite_float_literals_round_trip_and_use_exponents() {
        for number in [f64::MAX, f64::MIN, f64::MIN_POSITIVE, f64::from_bits(1), -0.0, 0.3] {
            let rendered = literal(&Value::Float(number)).unwrap();
            let inner = rendered.strip_prefix("toFloat64(").unwrap().strip_suffix(')').unwrap();
            assert!(inner.contains('e'), "{rendered}");
            assert_eq!(inner.parse::<f64>().unwrap().to_bits(), number.to_bits());
        }
    }

    #[test]
    fn nested_json_cell_edits_refuse_values_without_native_type_metadata() {
        assert!(matches!(
            literal(&Value::Json(Box::new(serde_json::json!([
                "18446744073709551616",
                null
            ])))),
            Err(DriverError::Unsupported(_))
        ));
    }
}
