use bookie_core::{Value, sql_dialect::BuildSqlError};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RowKey {
    Persisted(Vec<KeyValue>),
    Draft(u64),
}

impl RowKey {
    pub fn from_pk_values(pk_values: &[Value]) -> Option<Self> {
        if pk_values.is_empty() || pk_values_are_unreadable(pk_values) {
            return None;
        }
        Some(RowKey::Persisted(pk_values.iter().map(KeyValue::from).collect()))
    }
}

pub fn ambiguous_row_keys(rows: &[Vec<Value>], key_indices: &[usize]) -> std::collections::HashSet<RowKey> {
    let mut seen = std::collections::HashSet::new();
    let mut ambiguous = std::collections::HashSet::new();
    for row in rows {
        let values: Vec<Value> = key_indices
            .iter()
            .filter_map(|&index| row.get(index).cloned())
            .collect();
        if values.len() != key_indices.len() {
            continue;
        }
        if let Some(key) = RowKey::from_pk_values(&values)
            && !seen.insert(key.clone())
        {
            ambiguous.insert(key);
        }
    }
    ambiguous
}

pub fn pk_values_are_unreadable(pk_values: &[Value]) -> bool {
    pk_values.iter().any(|v| matches!(v, Value::Undecodable(_)))
}

#[derive(Debug, thiserror::Error)]
pub enum MaterializeError {
    #[error(transparent)]
    BuildSql(#[from] BuildSqlError),

    #[error("a row cannot be updated or deleted because its key value could not be read from the database")]
    UnreadableRowKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyValue {
    Null,
    Bool(bool),
    Int(i64),
    FloatBits(u64),
    Text(String),
    Bytes(Vec<u8>),
    Date(chrono::NaiveDate),
    Time(chrono::NaiveTime),
    DateTime(chrono::NaiveDateTime),
    TimestampTz(chrono::DateTime<chrono::Utc>),
    Decimal(String),
    Uuid(uuid::Uuid),
    Json(String),
    JsonText(String),
    Undecodable(String),
}

impl From<&Value> for KeyValue {
    fn from(v: &Value) -> Self {
        match v {
            Value::Null => KeyValue::Null,
            Value::Bool(b) => KeyValue::Bool(*b),
            Value::Int(i) => KeyValue::Int(*i),
            Value::Float(f) => KeyValue::FloatBits(f.to_bits()),
            Value::Text(s) => KeyValue::Text(s.clone()),
            Value::Bytes(b) => KeyValue::Bytes(b.clone()),
            Value::Date(d) => KeyValue::Date(*d),
            Value::Time(t) => KeyValue::Time(*t),
            Value::DateTime(dt) => KeyValue::DateTime(*dt),
            Value::TimestampTz(ts) => KeyValue::TimestampTz(*ts),
            Value::Decimal(d) => KeyValue::Decimal(d.to_string()),
            Value::Uuid(u) => KeyValue::Uuid(*u),
            Value::Json(j) => KeyValue::Json(j.to_string()),
            Value::JsonText(j) => KeyValue::JsonText(j.clone()),
            Value::Undecodable(type_name) => KeyValue::Undecodable(type_name.clone()),
        }
    }
}

pub(crate) fn keyvalue_to_value(kv: &KeyValue) -> Value {
    match kv {
        KeyValue::Null => Value::Null,
        KeyValue::Bool(b) => Value::Bool(*b),
        KeyValue::Int(i) => Value::Int(*i),
        KeyValue::FloatBits(bits) => Value::Float(f64::from_bits(*bits)),
        KeyValue::Text(s) => Value::Text(s.clone()),
        KeyValue::Bytes(b) => Value::Bytes(b.clone()),
        KeyValue::Date(d) => Value::Date(*d),
        KeyValue::Time(t) => Value::Time(*t),
        KeyValue::DateTime(dt) => Value::DateTime(*dt),
        KeyValue::TimestampTz(ts) => Value::TimestampTz(*ts),
        KeyValue::Decimal(s) => s.parse().map(Value::Decimal).unwrap_or(Value::Text(s.clone())),
        KeyValue::Uuid(u) => Value::Uuid(*u),
        KeyValue::Json(s) => serde_json::from_str(s)
            .map(Value::Json)
            .unwrap_or(Value::Text(s.clone())),
        KeyValue::JsonText(s) => Value::JsonText(s.clone()),
        KeyValue::Undecodable(type_name) => Value::Undecodable(type_name.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(a: i64, b: i64, payload: &str) -> Vec<Value> {
        vec![Value::Int(a), Value::Int(b), Value::Text(payload.into())]
    }

    #[test]
    fn rows_that_share_a_key_are_ambiguous() {
        let rows = vec![row(1, 1, "x"), row(1, 2, "y"), row(1, 1, "z")];

        let ambiguous = ambiguous_row_keys(&rows, &[0, 1]);

        assert_eq!(ambiguous.len(), 1);
        assert!(ambiguous.contains(&RowKey::from_pk_values(&[Value::Int(1), Value::Int(1)]).unwrap()));
    }

    #[test]
    fn a_partial_key_that_repeats_marks_those_rows() {
        let rows = vec![row(1, 1, "x"), row(1, 2, "y"), row(2, 1, "z")];

        let ambiguous = ambiguous_row_keys(&rows, &[0]);

        assert_eq!(
            ambiguous,
            std::collections::HashSet::from([RowKey::from_pk_values(&[Value::Int(1)]).unwrap()])
        );
    }

    #[test]
    fn unique_keys_unreadable_keys_and_no_key_columns_are_never_ambiguous() {
        let unique = vec![row(1, 1, "x"), row(1, 2, "y")];
        let unreadable = vec![
            vec![Value::Undecodable("a".into())],
            vec![Value::Undecodable("a".into())],
        ];

        assert!(ambiguous_row_keys(&unique, &[0, 1]).is_empty());
        assert!(ambiguous_row_keys(&unreadable, &[0]).is_empty());
        assert!(ambiguous_row_keys(&unique, &[]).is_empty());
    }
}
