use super::{placeholder_for, text_cast_placeholder};
use crate::{ColumnInfo, Value};

pub(super) fn exact_original_predicate(
    driver_id: &str,
    name: &str,
    column: &ColumnInfo,
    old_value: &Value,
    params: &mut Vec<Value>,
) -> Option<String> {
    let placeholder = placeholder_for(driver_id, params.len());
    let predicate = match (driver_id, old_value) {
        ("postgres", Value::Text(_)) if is_character(column) => {
            let value = text_cast_placeholder(column, old_value, placeholder);
            format!("{name}::text COLLATE \"C\" IS NOT DISTINCT FROM {value}::text COLLATE \"C\"")
        }
        ("postgres", Value::JsonText(_)) => {
            format!("{name}::text COLLATE \"C\" IS NOT DISTINCT FROM {placeholder}::text COLLATE \"C\"")
        }
        ("sqlite", Value::Text(_)) => format!("{name} COLLATE BINARY IS NOT DISTINCT FROM {placeholder}"),
        ("mysql", Value::Text(_)) if is_character(column) => format!(
            "CAST(CONVERT({name} USING utf8mb4) AS BINARY) <=> CAST(CONVERT({placeholder} USING utf8mb4) AS BINARY)"
        ),
        ("mssql", Value::Text(_)) if is_character(column) => {
            let text = format!("CONVERT(nvarchar(max), {name})");
            binary_predicate(&text, &format!("CONVERT(varbinary(max), {placeholder})"), &placeholder)
        }
        ("mssql", Value::Bytes(_)) => binary_predicate(name, &placeholder, &placeholder),
        _ => return None,
    };
    params.push(old_value.clone());
    Some(predicate)
}

fn binary_predicate(name: &str, binary_parameter: &str, parameter: &str) -> String {
    format!("(CONVERT(varbinary(max), {name}) = {binary_parameter} AND DATALENGTH({name}) = DATALENGTH({parameter}))")
}

fn is_character(column: &ColumnInfo) -> bool {
    let kind = column.data_type.trim().to_ascii_lowercase();
    let scalar = kind.strip_suffix("[]").unwrap_or(&kind);
    let base = scalar.split('(').next().unwrap_or_default().trim();
    column.collation.is_some()
        || matches!(
            base,
            "char"
                | "varchar"
                | "nchar"
                | "nvarchar"
                | "character"
                | "character varying"
                | "text"
                | "citext"
                | "bpchar"
                | "name"
                | "ntext"
                | "tinytext"
                | "mediumtext"
                | "longtext"
                | "enum"
                | "set"
        )
}
