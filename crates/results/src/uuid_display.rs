use bookie_core::{ColumnInfo, Value};
use uuid::Uuid;

pub fn format_binary_uuid(value: &Value, column: &ColumnInfo) -> Option<String> {
    let Value::Bytes(bytes) = value else {
        return None;
    };
    if bytes.len() != 16 || !is_mysql_binary_type(&column.data_type) {
        return None;
    }
    Uuid::from_slice(bytes).ok().map(|uuid| uuid.hyphenated().to_string())
}

fn is_mysql_binary_type(data_type: &str) -> bool {
    let (base_type, width) = data_type
        .split_once('(')
        .map_or((data_type, None), |(base, width)| (base, Some(width)));
    let is_binary =
        base_type.trim().eq_ignore_ascii_case("binary") || base_type.trim().eq_ignore_ascii_case("varbinary");
    is_binary && width.is_none_or(|width| width.trim_end_matches(')').trim().parse::<usize>().ok() == Some(16))
}

#[cfg(test)]
#[path = "uuid_display_tests.rs"]
mod tests;
