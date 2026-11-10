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

pub fn render_tsv(rows: impl IntoIterator<Item = Vec<Cell>>, with_headers: bool) -> String {
    bookie_core::export::render_tsv(
        &column_infos(),
        &QueryRows::from_rows(rows.into_iter().collect()),
        with_headers,
    )
}
