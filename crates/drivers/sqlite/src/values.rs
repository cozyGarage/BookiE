use super::*;

pub(crate) fn extract_value(row: &SqliteRow, idx: usize) -> Value {
    // SQLite's non-optional decoders turn NULL into zero/empty values.
    // Inspect the storage value before using the declared column type.
    match row.try_get_raw(idx) {
        Ok(value) if value.is_null() => return Value::Null,
        Err(_) => return Value::Undecodable(row.columns()[idx].type_info().name().to_string()),
        Ok(_) => {}
    }
    let type_name = row.columns()[idx].type_info().name().to_ascii_uppercase();
    let runtime_type = row
        .try_get_raw(idx)
        .map(|value| value.type_info().name().to_ascii_uppercase())
        .unwrap_or_default();
    match type_name.as_str() {
        "INTEGER" => row
            .try_get::<i64, _>(idx)
            .map(Value::Int)
            .unwrap_or_else(|_| decode_fallback(row, idx)),
        "REAL" => row
            .try_get::<f64, _>(idx)
            .map(Value::Float)
            .unwrap_or_else(|_| decode_fallback(row, idx)),
        "BLOB" => match runtime_type.as_str() {
            "TEXT" => row
                .try_get::<String, _>(idx)
                .map(Value::Text)
                .unwrap_or_else(|_| decode_fallback(row, idx)),
            "INTEGER" => row
                .try_get::<i64, _>(idx)
                .map(Value::Int)
                .unwrap_or_else(|_| decode_fallback(row, idx)),
            "REAL" => row
                .try_get::<f64, _>(idx)
                .map(Value::Float)
                .unwrap_or_else(|_| decode_fallback(row, idx)),
            _ => row
                .try_get::<Vec<u8>, _>(idx)
                .map(Value::Bytes)
                .unwrap_or_else(|_| decode_fallback(row, idx)),
        },
        "BOOLEAN" => row
            .try_get::<bool, _>(idx)
            .map(|flag| boolean_value(flag, row.try_get_unchecked::<i64, _>(idx).ok()))
            .unwrap_or_else(|_| decode_fallback(row, idx)),
        "DATE" | "TIME" | "DATETIME" | "TIMESTAMP" => {
            decode_declared_temporal(row, idx, type_name.as_str(), runtime_type.as_str())
        }
        _ => untyped_value(row, idx),
    }
}

pub(crate) fn decode_declared_temporal(row: &SqliteRow, idx: usize, declared_type: &str, runtime_type: &str) -> Value {
    match runtime_type {
        "INTEGER" => row
            .try_get::<i64, _>(idx)
            .map(Value::Int)
            .unwrap_or_else(|_| decode_fallback(row, idx)),
        "REAL" => row
            .try_get::<f64, _>(idx)
            .map(Value::Float)
            .unwrap_or_else(|_| decode_fallback(row, idx)),
        "BLOB" => row
            .try_get::<Vec<u8>, _>(idx)
            .map(Value::Bytes)
            .unwrap_or_else(|_| decode_fallback(row, idx)),
        "TEXT" => decode_temporal_text(row, idx, declared_type),
        _ => decode_fallback(row, idx),
    }
}

pub(crate) fn decode_temporal_text(row: &SqliteRow, idx: usize, declared_type: &str) -> Value {
    let Ok(text) = row.try_get::<String, _>(idx) else {
        return decode_fallback(row, idx);
    };
    if temporal_text_has_offset(&text, declared_type) {
        return Value::Text(text);
    }
    match declared_type {
        "DATE" => row
            .try_get::<chrono::NaiveDate, _>(idx)
            .map(Value::Date)
            .unwrap_or(Value::Text(text)),
        "TIME" => row
            .try_get::<chrono::NaiveTime, _>(idx)
            .map(Value::Time)
            .unwrap_or(Value::Text(text)),
        "DATETIME" | "TIMESTAMP" => row
            .try_get::<chrono::NaiveDateTime, _>(idx)
            .map(Value::DateTime)
            .unwrap_or(Value::Text(text)),
        _ => Value::Text(text),
    }
}

pub(crate) fn temporal_text_has_offset(text: &str, declared_type: &str) -> bool {
    if text.ends_with('Z') || text.ends_with('z') {
        return true;
    }
    let time_start = match declared_type {
        "TIME" => 0,
        "DATETIME" | "TIMESTAMP" => text.find(['T', ' ']).map_or(text.len(), |index| index + 1),
        _ => return false,
    };
    text.get(time_start..)
        .is_some_and(|time| time.contains('+') || time.contains('-'))
}

/// A column with no declared type - every expression, aggregate and
/// `count(*)` - reaches here. SQLite decides the value's type at
/// runtime, so the decode is tried in the order the storage classes can
/// hold, and `Option` keeps a real NULL distinct from a type mismatch.
pub(crate) fn untyped_value(row: &SqliteRow, idx: usize) -> Value {
    if let Ok(value) = row.try_get::<Option<i64>, _>(idx) {
        return value.map_or(Value::Null, Value::Int);
    }
    if let Ok(value) = row.try_get::<Option<f64>, _>(idx) {
        return value.map_or(Value::Null, Value::Float);
    }
    if let Ok(value) = row.try_get::<Option<String>, _>(idx) {
        return value.map_or(Value::Null, Value::Text);
    }
    row.try_get::<Vec<u8>, _>(idx)
        .map(Value::Bytes)
        .unwrap_or_else(|_| Value::Undecodable(row.columns()[idx].type_info().name().to_string()))
}

pub(crate) fn sqlite_row_payload_bytes(row: &SqliteRow) -> usize {
    (0..row.len()).fold(0usize, |total, index| {
        let length = match row.try_get_raw(index) {
            Ok(value) if value.is_null() => 0,
            Ok(value) => match value.type_info().name() {
                "TEXT" => row
                    .try_get::<&str, _>(index)
                    .map(str::len)
                    .or_else(|_| row.try_get::<&[u8], _>(index).map(<[u8]>::len))
                    .unwrap_or(usize::MAX),
                "BLOB" => row.try_get::<&[u8], _>(index).map_or(usize::MAX, <[u8]>::len),
                _ => 0,
            },
            Err(_) => usize::MAX,
        };
        total.saturating_add(length)
    })
}

/// A declared column type is a hint, not a guarantee -- SQLite's type
/// affinity can still store a row's actual value in a different
/// storage class than the column's declared type. When the declared
/// type's own decode fails, this is reached to find out whether the
/// value is genuinely NULL or just stored as text/bytes instead, so a
/// real value is never silently shown as an empty cell.
pub(crate) fn boolean_value(flag: bool, stored: Option<i64>) -> Value {
    match (flag, stored) {
        (true, Some(other)) if other != 1 => Value::Int(other),
        _ => Value::Bool(flag),
    }
}

pub(crate) fn decode_fallback(row: &SqliteRow, idx: usize) -> Value {
    untyped_value(row, idx)
}

pub(crate) fn bind_sqlite_params<'q>(
    mut q: sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments>,
    params: &'q [Value],
) -> Result<sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments>, DriverError> {
    for p in params {
        q = match p {
            Value::Null => q.bind(Option::<&str>::None),
            Value::Bool(b) => q.bind(*b),
            Value::Int(i) => q.bind(*i),
            Value::Float(f) if f.is_nan() || (*f == 0.0 && f.is_sign_negative()) => {
                return Err(DriverError::Unsupported(
                    "SQLite cannot preserve NaN or negative zero as a REAL value".into(),
                ));
            }
            Value::Float(f) => q.bind(*f),
            Value::Text(s) => q.bind(s.clone()),
            Value::JsonText(s) => q.bind(s.clone()),
            Value::Bytes(b) => q.bind(b.clone()),
            Value::Date(d) => q.bind(*d),
            Value::Time(t) => q.bind(*t),
            Value::DateTime(dt) => q.bind(*dt),
            Value::TimestampTz(ts) => q.bind(*ts),
            Value::Decimal(d) => match exact_real(d) {
                Some(real) => q.bind(real),
                None => q.bind(d.to_string()),
            },
            Value::Uuid(u) => q.bind(u.to_string()),
            Value::Json(j) => q.bind(j.to_string()),
            Value::Undecodable(_) => {
                return Err(DriverError::Unsupported(
                    "undecodable cell cannot be bound as a parameter".into(),
                ));
            }
        };
    }
    Ok(q)
}

pub(crate) fn exact_real(decimal: &rust_decimal::Decimal) -> Option<f64> {
    let digits = decimal.normalize().to_string();
    digits.parse::<f64>().ok().filter(|real| real.to_string() == digits)
}
