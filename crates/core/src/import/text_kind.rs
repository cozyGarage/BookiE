use crate::ColumnInfo;
use crate::import::cell::{ColumnKind, column_kind};

pub(crate) fn column_kind_for(column: &ColumnInfo, driver_id: &str) -> ColumnKind {
    if column.enum_type.is_some() || column.domain_type.is_some() {
        return ColumnKind::Text;
    }
    let lowered = column.data_type.trim().to_ascii_lowercase();
    let is_range = lowered.ends_with("range") || lowered.contains("multirange");
    let is_bit_string = driver_id == "postgres" && lowered.starts_with("bit");
    if is_range || is_bit_string {
        return ColumnKind::Text;
    }
    if driver_id == "mysql" && (lowered == "timestamp" || lowered.starts_with("timestamp(")) {
        return ColumnKind::TimestampTz;
    }
    if is_unsigned_64(column, driver_id) {
        return ColumnKind::Text;
    }
    column_kind(&column.data_type)
}

pub(super) fn is_unsigned_64(column: &ColumnInfo, driver_id: &str) -> bool {
    let lowered = column.data_type.trim().to_ascii_lowercase();
    let base = lowered
        .strip_prefix("nullable(")
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(&lowered);
    match driver_id {
        "mysql" => lowered.starts_with("bigint") && lowered.split_whitespace().any(|part| part == "unsigned"),
        "duckdb" => lowered == "ubigint",
        "clickhouse" => base == "uint64",
        _ => false,
    }
}
