use super::{placeholder_for, postgres_text_cast_type};
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
        ("postgres", Value::Text(_)) if postgres_character_output(column, old_value) => {
            let value = match postgres_text_cast_type(column, old_value) {
                Some(kind) => format!("{placeholder}::text::{kind}"),
                None if column.data_type.trim().ends_with("[]") => {
                    format!("CASE WHEN FALSE THEN {name} ELSE {placeholder} END")
                }
                None => format!("{placeholder}::text"),
            };
            format!(
                "({name} IS NOT NULL AND pg_catalog.format('%s', {name}) COLLATE pg_catalog.\"C\" = pg_catalog.format('%s', {value}) COLLATE pg_catalog.\"C\")"
            )
        }
        ("postgres", Value::JsonText(_)) => {
            format!(
                "{name}::text COLLATE pg_catalog.\"C\" IS NOT DISTINCT FROM {placeholder}::text COLLATE pg_catalog.\"C\""
            )
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

fn postgres_character_output(column: &ColumnInfo, old_value: &Value) -> bool {
    is_character(column)
        || column.domain_type.is_some()
        || (column.data_type.trim().ends_with("[]") && postgres_text_cast_type(column, old_value).is_none())
}
