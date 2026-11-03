use std::borrow::Cow;

use bookie_core::Value;
use thiserror::Error;

use crate::change_tracker::KeyValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnScope {
    Any,
    Column(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResultPredicate {
    Contains { scope: ColumnScope, needle: String },
    Equals { column: usize, value: Value },
    NotEquals { column: usize, value: Value },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PredicateError {
    #[error("the search text is empty")]
    EmptyNeedle,
    #[error("the column is not in the result")]
    UnknownColumn,
    #[error("the value cannot be compared exactly")]
    UncomparableValue,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CellTally {
    pub unsearchable_cells: u64,
}

#[derive(Debug, Clone)]
pub struct PreparedPredicate {
    kind: PreparedKind,
}

#[derive(Debug, Clone)]
enum PreparedKind {
    Contains { scope: ColumnScope, needle: String },
    Equals { column: usize, key: KeyValue },
    NotEquals { column: usize, key: KeyValue },
}

impl ResultPredicate {
    pub fn prepare(self, column_count: usize) -> Result<PreparedPredicate, PredicateError> {
        let kind = match self {
            Self::Contains { scope, needle } => prepare_contains(scope, needle, column_count)?,
            Self::Equals { column, value } => PreparedKind::Equals {
                column: checked_column(column, column_count)?,
                key: comparable_key(&value)?,
            },
            Self::NotEquals { column, value } => PreparedKind::NotEquals {
                column: checked_column(column, column_count)?,
                key: comparable_key(&value)?,
            },
        };
        Ok(PreparedPredicate { kind })
    }
}

fn prepare_contains(scope: ColumnScope, needle: String, column_count: usize) -> Result<PreparedKind, PredicateError> {
    if needle.is_empty() {
        return Err(PredicateError::EmptyNeedle);
    }
    if let ColumnScope::Column(column) = scope {
        checked_column(column, column_count)?;
    }
    Ok(PreparedKind::Contains {
        scope,
        needle: needle.to_lowercase(),
    })
}

fn checked_column(column: usize, column_count: usize) -> Result<usize, PredicateError> {
    (column < column_count)
        .then_some(column)
        .ok_or(PredicateError::UnknownColumn)
}

fn comparable_key(value: &Value) -> Result<KeyValue, PredicateError> {
    if matches!(value, Value::Undecodable(_)) {
        return Err(PredicateError::UncomparableValue);
    }
    Ok(KeyValue::from(value))
}

impl PreparedPredicate {
    pub fn matches(&self, row: &[Value], tally: &mut CellTally) -> bool {
        match &self.kind {
            PreparedKind::Contains { scope, needle } => contains_in_row(row, scope, needle, tally),
            PreparedKind::Equals { column, key } => cell_key(row, *column).is_some_and(|cell| &cell == key),
            PreparedKind::NotEquals { column, key } => cell_key(row, *column).is_none_or(|cell| &cell != key),
        }
    }
}

fn cell_key(row: &[Value], column: usize) -> Option<KeyValue> {
    row.get(column).map(KeyValue::from)
}

fn contains_in_row(row: &[Value], scope: &ColumnScope, needle: &str, tally: &mut CellTally) -> bool {
    match scope {
        ColumnScope::Any => row.iter().any(|cell| cell_contains(cell, needle, tally)),
        ColumnScope::Column(column) => row.get(*column).is_some_and(|cell| cell_contains(cell, needle, tally)),
    }
}

fn cell_contains(cell: &Value, needle: &str, tally: &mut CellTally) -> bool {
    match search_text(cell) {
        SearchText::Text(text) => contains_ignore_case(&text, needle),
        SearchText::Never => false,
        SearchText::Unsearchable => {
            tally.unsearchable_cells = tally.unsearchable_cells.saturating_add(1);
            false
        }
    }
}

enum SearchText<'a> {
    Text(Cow<'a, str>),
    Never,
    Unsearchable,
}

fn search_text(value: &Value) -> SearchText<'_> {
    match value {
        Value::Null => SearchText::Never,
        Value::Undecodable(_) => SearchText::Unsearchable,
        Value::Text(text) | Value::JsonText(text) => SearchText::Text(Cow::Borrowed(text)),
        Value::Bool(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Int(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Float(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Decimal(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Date(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Time(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::DateTime(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::TimestampTz(value) => SearchText::Text(Cow::Owned(value.to_rfc3339())),
        Value::Uuid(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Json(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Bytes(bytes) => SearchText::Text(Cow::Owned(hex_text(bytes))),
    }
}

fn hex_text(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn contains_ignore_case(haystack: &str, needle_lower: &str) -> bool {
    if haystack.is_ascii() && needle_lower.is_ascii() {
        let needle = needle_lower.as_bytes();
        return haystack
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle));
    }
    haystack.to_lowercase().contains(needle_lower)
}

#[cfg(test)]
mod tests;
