use super::BuildSqlError;
use crate::{ColumnInfo, Value};
use chrono::Datelike;

pub(crate) fn is_clickhouse_date_type(driver_id: &str, data_type: &str) -> bool {
    if !driver_id.eq_ignore_ascii_case("clickhouse") {
        return false;
    }
    let mut current = data_type.trim();
    while let Some((outer, inner)) = current.split_once('(') {
        if !outer.trim().eq_ignore_ascii_case("nullable") && !outer.trim().eq_ignore_ascii_case("lowcardinality") {
            break;
        }
        let Some(inner) = inner.trim().strip_suffix(')') else {
            break;
        };
        current = inner.trim();
    }
    current.eq_ignore_ascii_case("date")
}

pub(crate) fn validate_write_value(driver_id: &str, column: &ColumnInfo, value: &Value) -> Result<(), BuildSqlError> {
    if !is_clickhouse_date_type(driver_id, &column.data_type) || matches!(value, Value::Null) {
        return Ok(());
    }
    let date = match value {
        Value::Date(date) => Some(*date),
        Value::Text(text) => chrono::NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok(),
        _ => None,
    };
    if date.is_some_and(|date| ((1970, 1, 1)..=(2149, 6, 6)).contains(&(date.year(), date.month(), date.day()))) {
        Ok(())
    } else {
        Err(BuildSqlError::ClickhouseDateOutOfRange {
            column: column.name.clone(),
        })
    }
}
