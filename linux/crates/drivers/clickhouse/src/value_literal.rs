use tablepro_core::{DriverError, Value};

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
                f.to_string()
            }
        }
        Value::Text(s) => format!("'{}'", escape_str(s)),
        Value::Bytes(b) => format!("unhex('{}')", hex_encode(b)),
        Value::Date(d) => format!("toDate('{}')", d.format("%Y-%m-%d")),
        Value::Time(t) => format!("'{}'", t.format("%H:%M:%S%.f")),
        Value::DateTime(dt) if !tablepro_core::sql_literal::clickhouse_datetime64_nanos_supported(*dt) => {
            return Err(unsupported_datetime64_range());
        }
        Value::DateTime(dt) => format!("toDateTime64('{}', 9)", dt.format("%Y-%m-%d %H:%M:%S%.9f")),
        Value::TimestampTz(ts)
            if !tablepro_core::sql_literal::clickhouse_datetime64_nanos_supported(ts.naive_utc()) =>
        {
            return Err(unsupported_datetime64_range());
        }
        Value::TimestampTz(ts) => format!("toDateTime64('{}', 9, 'UTC')", ts.format("%Y-%m-%d %H:%M:%S%.9f")),
        Value::Decimal(d) => format!("toDecimal128('{d}', {})", d.scale()),
        Value::Uuid(u) => format!("toUUID('{u}')"),
        Value::Json(_) => {
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
        "ClickHouse DateTime64(9) supports timestamps from 1900-01-01 through 2262-04-11 23:47:16.854775807".into(),
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
    use tablepro_core::{DriverError, Value};

    #[test]
    fn nested_json_cell_edits_refuse_values_without_native_type_metadata() {
        assert!(matches!(
            literal(&Value::Json(serde_json::json!(["18446744073709551616", null]))),
            Err(DriverError::Unsupported(_))
        ));
    }
}
