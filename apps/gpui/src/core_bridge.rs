use bookie_core::import::{ColumnKind, parse_cell};
use bookie_core::{ColumnInfo, FilterValueKind, Value};

use crate::model::{Cell, ColumnMeta};

fn column_kind(kind: &str) -> Option<ColumnKind> {
    Some(match kind {
        "boolean" => ColumnKind::Bool,
        "integer" => ColumnKind::Int,
        "float" => ColumnKind::Float,
        "decimal" => ColumnKind::Decimal,
        "date" => ColumnKind::Date,
        "time" => ColumnKind::Time,
        "datetime" => ColumnKind::DateTime,
        "timestamptz" => ColumnKind::TimestampTz,
        "uuid" => ColumnKind::Uuid,
        "json" => ColumnKind::Json,
        "bytes" => ColumnKind::Bytes,
        "text" | "json_text" | "null" => ColumnKind::Text,
        _ => return None,
    })
}

/// The edit-kind name that `parse_edit` understands for a core filter kind.
pub fn edit_kind(kind: FilterValueKind) -> &'static str {
    match kind {
        FilterValueKind::Text => "text",
        FilterValueKind::Bool => "boolean",
        FilterValueKind::Int => "integer",
        FilterValueKind::Float => "float",
        FilterValueKind::Decimal => "decimal",
        FilterValueKind::Date => "date",
        FilterValueKind::Time => "time",
        FilterValueKind::DateTime => "datetime",
        FilterValueKind::TimestampTz => "timestamptz",
        FilterValueKind::Uuid => "uuid",
        FilterValueKind::Json => "json",
        FilterValueKind::Bytes => "bytes",
    }
}

pub fn parse_edit(kind: &str, text: &str) -> Result<Cell, String> {
    let column = column_kind(kind).ok_or("this cell cannot be edited")?;
    let trimmed = if matches!(kind, "text" | "json_text" | "null") {
        text
    } else {
        text.trim()
    };
    let value = parse_cell(trimmed, column).map_err(|error| error.to_string())?;
    Ok(match (kind, value) {
        ("json_text", Value::Text(text)) => Value::JsonText(text),
        (_, value) => value,
    })
}

pub fn column_infos() -> Vec<ColumnInfo> {
    ["value:integer", "label:text", "state:boolean", "typed:variant"]
        .iter()
        .map(|spec| {
            let (name, data_type) = spec.split_once(':').unwrap_or((spec, "text"));
            ColumnInfo {
                name: name.into(),
                data_type: data_type.into(),
                nullable: true,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            }
        })
        .collect()
}

/// The core column description for a result's columns. Types are not carried by the grid's
/// columns, so the declared type is left empty.
pub fn column_infos_for(columns: &[ColumnMeta]) -> Vec<ColumnInfo> {
    columns
        .iter()
        .map(|column| ColumnInfo {
            name: column.name.clone(),
            data_type: String::new(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        })
        .collect()
}
