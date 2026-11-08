use bookie_core::Value;
use bookie_core::import::{ColumnKind, column_kind};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::enum_input::declared_member_labels;

pub const MIN_CELLS: u32 = 4;
pub const TEXT_MAX_CELLS: u32 = 40;
pub const TYPICAL_TEXT_CELLS: u32 = 20;
pub const TYPICAL_DIGIT_CELLS: u32 = 10;
pub const BOOL_CELLS: u32 = 5;
pub const SAMPLE_ROWS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthKind {
    Bool,
    Number,
    Temporal,
    Text,
}

pub fn width_kind_for_data_type(data_type: &str) -> WidthKind {
    if is_mysql_set(data_type) {
        return WidthKind::Text;
    }
    if declared_member_labels(data_type).is_some() {
        return WidthKind::Text;
    }
    match column_kind(data_type) {
        ColumnKind::Bool => WidthKind::Bool,
        ColumnKind::Int | ColumnKind::Float | ColumnKind::Decimal => WidthKind::Number,
        ColumnKind::Date | ColumnKind::Time | ColumnKind::DateTime | ColumnKind::TimestampTz => WidthKind::Temporal,
        ColumnKind::Uuid | ColumnKind::Json | ColumnKind::Bytes | ColumnKind::Text => WidthKind::Text,
    }
}

pub fn width_kind_for_value(value: &Value) -> WidthKind {
    match value {
        Value::Bool(_) => WidthKind::Bool,
        Value::Int(_) | Value::Float(_) | Value::Decimal(_) => WidthKind::Number,
        Value::Date(_) | Value::Time(_) | Value::DateTime(_) | Value::TimestampTz(_) => WidthKind::Temporal,
        Value::Null
        | Value::Text(_)
        | Value::Bytes(_)
        | Value::Uuid(_)
        | Value::Json(_)
        | Value::JsonText(_)
        | Value::Undecodable(_) => WidthKind::Text,
    }
}

pub fn monospace_cells(text: &str) -> u32 {
    u32::try_from(text.width()).unwrap_or(u32::MAX)
}

pub fn is_plain_ascii(text: &str) -> bool {
    text.chars()
        .all(|character| character == ' ' || character.is_ascii_graphic())
}

pub fn within_cell_cap(text: &str) -> &str {
    let cap = usize::try_from(TEXT_MAX_CELLS).unwrap_or(usize::MAX);
    let mut taken = 0;
    for (at, character) in text.char_indices() {
        taken += character.width().unwrap_or(0);
        if taken > cap {
            return &text[..at];
        }
    }
    text
}

pub fn header_measure_text(name: &str) -> &str {
    within_cell_cap(name)
}

pub fn default_width_cells<'a>(data_type: &str, sample_texts: impl IntoIterator<Item = &'a str>) -> u32 {
    let kind = width_kind_for_data_type(data_type);
    let widest = widest_cells(sample_texts);
    let cells = match kind {
        WidthKind::Bool => BOOL_CELLS,
        WidthKind::Number => widest,
        WidthKind::Temporal => widest.max(spelled_floor(data_type)),
        WidthKind::Text => widest.min(TEXT_MAX_CELLS),
    };
    cells.max(MIN_CELLS)
}

fn spelled_floor(data_type: &str) -> u32 {
    spelled(column_kind(data_type))
        .into_iter()
        .map(monospace_cells)
        .max()
        .unwrap_or(0)
}

pub fn number_width_cells<'a>(sample_texts: impl IntoIterator<Item = &'a str>) -> u32 {
    widest_cells(sample_texts).max(MIN_CELLS)
}

pub fn typical_width_cells(data_type: &str) -> u32 {
    let typical = if is_mysql_set(data_type) {
        TYPICAL_TEXT_CELLS
    } else if let Some(labels) = declared_member_labels(data_type) {
        labels
            .iter()
            .map(|label| monospace_cells(label))
            .max()
            .unwrap_or(TYPICAL_TEXT_CELLS)
    } else {
        typical_for_kind(data_type)
    };
    typical.clamp(MIN_CELLS, TEXT_MAX_CELLS)
}

pub fn typical_measure_texts(data_type: &str) -> Vec<String> {
    if is_mysql_set(data_type) {
        return Vec::new();
    }
    if let Some(labels) = declared_member_labels(data_type) {
        return labels
            .into_iter()
            .take(SAMPLE_ROWS)
            .map(|label| within_cell_cap(&label).to_owned())
            .collect();
    }
    spelled(column_kind(data_type))
        .into_iter()
        .map(|text| within_cell_cap(text).to_owned())
        .collect()
}

pub fn sampled_measure_texts<'a>(kind: WidthKind, sample_texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    if kind == WidthKind::Number {
        return Vec::new();
    }
    let mut widest_plain: Option<(u32, String)> = None;
    let mut others = Vec::new();
    for text in sample_texts.into_iter().take(SAMPLE_ROWS) {
        let text = within_cell_cap(text);
        if text.is_empty() {
            continue;
        }
        if !is_plain_ascii(text) {
            others.push(text.to_owned());
            continue;
        }
        let width = monospace_cells(text);
        if widest_plain.as_ref().is_none_or(|(most, _)| width > *most) {
            widest_plain = Some((width, text.to_owned()));
        }
    }
    others.sort_unstable();
    others.dedup();
    others.extend(widest_plain.map(|(_, text)| text));
    others
}

fn widest_cells<'a>(sample_texts: impl IntoIterator<Item = &'a str>) -> u32 {
    sample_texts
        .into_iter()
        .take(SAMPLE_ROWS)
        .filter(|text| !text.is_empty())
        .map(monospace_cells)
        .max()
        .unwrap_or(0)
}

fn typical_for_kind(data_type: &str) -> u32 {
    match column_kind(data_type) {
        ColumnKind::Bool => BOOL_CELLS,
        ColumnKind::Decimal => decimal_typical(data_type),
        ColumnKind::Int | ColumnKind::Float => TYPICAL_DIGIT_CELLS,
        ColumnKind::Text => text_bound(data_type)
            .unwrap_or(TYPICAL_TEXT_CELLS)
            .min(TYPICAL_TEXT_CELLS),
        other => spelled(other)
            .into_iter()
            .map(monospace_cells)
            .max()
            .unwrap_or(TYPICAL_TEXT_CELLS),
    }
}

fn decimal_typical(data_type: &str) -> u32 {
    match first_type_argument(data_type) {
        Some(precision) => precision.saturating_add(2).min(TYPICAL_TEXT_CELLS),
        None => TYPICAL_DIGIT_CELLS,
    }
}

fn text_bound(data_type: &str) -> Option<u32> {
    let lower = data_type.trim().to_ascii_lowercase();
    let base = lower.split('(').next().unwrap_or("").trim();
    let character = matches!(
        base,
        "varchar" | "character varying" | "nvarchar" | "char" | "character" | "nchar" | "bpchar" | "varying"
    );
    if !character {
        return None;
    }
    first_type_argument(data_type)
}

fn first_type_argument(data_type: &str) -> Option<u32> {
    let open = data_type.find('(')?;
    let close = data_type[open + 1..].find(')')?;
    let inside = data_type[open + 1..open + 1 + close].trim();
    let token = inside.split(',').next()?.trim();
    if token.eq_ignore_ascii_case("max") {
        return None;
    }
    token.parse().ok()
}

fn is_mysql_set(data_type: &str) -> bool {
    let trimmed = data_type.trim();
    let Some(open) = trimmed.find('(') else {
        return trimmed.eq_ignore_ascii_case("set") || trimmed.to_ascii_lowercase().starts_with("set ");
    };
    trimmed[..open].trim().eq_ignore_ascii_case("set")
}

fn spelled(kind: ColumnKind) -> Vec<&'static str> {
    match kind {
        ColumnKind::Date => vec!["0000-00-00"],
        ColumnKind::Time => vec!["00:00:00"],
        ColumnKind::DateTime => vec!["0000-00-00 00:00:00"],
        ColumnKind::TimestampTz => vec!["0000-00-00 00:00:00+00:00"],
        ColumnKind::Uuid => vec!["00000000-0000-0000-0000-000000000000"],
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[path = "column_width_policy_tests.rs"]
mod tests;
