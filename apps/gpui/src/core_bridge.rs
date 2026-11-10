use bookie_core::import::{ColumnKind, parse_cell};
use bookie_core::{ColumnInfo, QueryRows, Value};

use crate::model::Cell;

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

pub fn cell_to_value(cell: &Cell) -> Value {
    match cell {
        Cell::Exact(value) => value.as_ref().clone(),
        Cell::Null => Value::Null,
        Cell::Bool(value) => Value::Bool(*value),
        Cell::Int(value) => Value::Int(*value),
        Cell::Float(value) => Value::Float(*value),
        Cell::Text(value) => Value::Text(value.clone()),
        Cell::JsonText(value) => Value::JsonText(value.clone()),
        Cell::Undecodable(type_name) => Value::Undecodable(type_name.clone()),
        Cell::Bytes(value) => Value::Bytes(value.clone()),
        Cell::Date(text) => reparse(ColumnKind::Date, text),
        Cell::Time(text) => reparse(ColumnKind::Time, text),
        Cell::DateTime(text) => reparse(ColumnKind::DateTime, text),
        Cell::TimestampTz(text) => reparse(ColumnKind::TimestampTz, text),
        Cell::Decimal(text) => reparse(ColumnKind::Decimal, text),
        Cell::Uuid(text) => reparse(ColumnKind::Uuid, text),
        Cell::Json(text) => reparse(ColumnKind::Json, text),
    }
}

fn reparse(kind: ColumnKind, text: &str) -> Value {
    parse_cell(text, kind).unwrap_or_else(|_| Value::Text(text.to_string()))
}

pub fn value_to_cell(value: Value) -> Cell {
    match value {
        Value::Date(_)
        | Value::Time(_)
        | Value::DateTime(_)
        | Value::TimestampTz(_)
        | Value::Decimal(_)
        | Value::Uuid(_)
        | Value::Json(_) => Cell::Exact(std::sync::Arc::new(value)),
        other => edit_value_to_cell(other),
    }
}

fn edit_value_to_cell(value: Value) -> Cell {
    match value {
        Value::Null => Cell::Null,
        Value::Bool(value) => Cell::Bool(value),
        Value::Int(value) => Cell::Int(value),
        Value::Float(value) => Cell::Float(value),
        Value::Text(value) => Cell::Text(value),
        Value::Bytes(value) => Cell::Bytes(value),
        Value::Date(value) => Cell::Date(value.to_string()),
        Value::Time(value) => Cell::Time(value.to_string()),
        Value::DateTime(value) => Cell::DateTime(value.to_string()),
        Value::TimestampTz(value) => Cell::TimestampTz(value.to_rfc3339()),
        Value::Decimal(value) => Cell::Decimal(value.to_string()),
        Value::Uuid(value) => Cell::Uuid(value.to_string()),
        Value::Json(value) => Cell::Json(value.to_string()),
        Value::JsonText(value) => Cell::JsonText(value),
        Value::Undecodable(type_name) => Cell::Undecodable(type_name),
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
    Ok(match (kind, edit_value_to_cell(value)) {
        ("json_text", Cell::Text(text)) => Cell::JsonText(text),
        (_, cell) => cell,
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

pub fn render_tsv(rows: impl IntoIterator<Item = Vec<Cell>>, with_headers: bool) -> String {
    let rows: Vec<Vec<Value>> = rows
        .into_iter()
        .map(|row| row.iter().map(cell_to_value).collect())
        .collect();
    bookie_core::export::render_tsv(&column_infos(), &QueryRows::from_rows(rows), with_headers)
}
