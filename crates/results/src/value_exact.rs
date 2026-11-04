use bookie_core::Value;

pub fn exact_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
        (Value::Text(a), Value::Text(b)) | (Value::JsonText(a), Value::JsonText(b)) => a == b,
        (Value::Bytes(a), Value::Bytes(b)) => a == b,
        (Value::Date(a), Value::Date(b)) => a == b,
        (Value::Time(a), Value::Time(b)) => a == b,
        (Value::DateTime(a), Value::DateTime(b)) => a == b,
        (Value::TimestampTz(a), Value::TimestampTz(b)) => a == b,
        (Value::Decimal(a), Value::Decimal(b)) => a == b && a.scale() == b.scale(),
        (Value::Uuid(a), Value::Uuid(b)) => a == b,
        (Value::Json(a), Value::Json(b)) => a.to_string() == b.to_string(),
        (Value::Undecodable(a), Value::Undecodable(b)) => a == b,
        _ => false,
    }
}
