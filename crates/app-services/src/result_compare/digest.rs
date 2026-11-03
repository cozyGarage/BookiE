use bookie_core::Value;
use chrono::{Datelike, Timelike};
use sha2::{Digest as _, Sha256};

pub(crate) type Digest = [u8; 16];

pub(crate) fn cells_digest(row: &[Value], indices: &[usize]) -> Digest {
    let mut hasher = Sha256::new();
    for &index in indices {
        match row.get(index) {
            Some(value) => encode(&mut hasher, value),
            None => hasher.update([0_u8]),
        }
    }
    let full = hasher.finalize();
    let mut digest = [0_u8; 16];
    digest.copy_from_slice(&full[..16]);
    digest
}

pub(crate) fn has_unreadable(row: &[Value], indices: &[usize]) -> bool {
    indices
        .iter()
        .any(|&index| matches!(row.get(index), Some(Value::Undecodable(_))))
}

pub(crate) fn has_null_or_unreadable(row: &[Value], indices: &[usize]) -> bool {
    indices
        .iter()
        .any(|&index| matches!(row.get(index), Some(Value::Null | Value::Undecodable(_)) | None))
}

fn encode(hasher: &mut Sha256, value: &Value) {
    match value {
        Value::Null => hasher.update([0_u8]),
        Value::Bool(value) => hasher.update([1, u8::from(*value)]),
        Value::Int(value) => tagged(hasher, 2, &value.to_be_bytes()),
        Value::Float(value) => tagged(hasher, 3, &value.to_bits().to_be_bytes()),
        Value::Text(value) => sized(hasher, 4, value.as_bytes()),
        Value::Bytes(value) => sized(hasher, 5, value),
        Value::Date(value) => tagged(hasher, 6, &value.num_days_from_ce().to_be_bytes()),
        Value::Time(value) => {
            tagged(hasher, 7, &value.num_seconds_from_midnight().to_be_bytes());
            hasher.update(value.nanosecond().to_be_bytes());
        }
        Value::DateTime(value) => {
            tagged(hasher, 8, &value.date().num_days_from_ce().to_be_bytes());
            hasher.update(value.time().num_seconds_from_midnight().to_be_bytes());
            hasher.update(value.time().nanosecond().to_be_bytes());
        }
        Value::TimestampTz(value) => {
            tagged(hasher, 9, &value.timestamp().to_be_bytes());
            hasher.update(value.timestamp_subsec_nanos().to_be_bytes());
        }
        Value::Decimal(value) => sized(hasher, 10, value.to_string().as_bytes()),
        Value::Uuid(value) => tagged(hasher, 11, value.as_bytes()),
        Value::Json(value) => sized(hasher, 12, value.to_string().as_bytes()),
        Value::JsonText(value) => sized(hasher, 13, value.as_bytes()),
        Value::Undecodable(value) => sized(hasher, 14, value.as_bytes()),
    }
}

fn tagged(hasher: &mut Sha256, tag: u8, bytes: &[u8]) {
    hasher.update([tag]);
    hasher.update(bytes);
}

fn sized(hasher: &mut Sha256, tag: u8, bytes: &[u8]) {
    hasher.update([tag]);
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}
