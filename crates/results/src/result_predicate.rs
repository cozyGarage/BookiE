use std::borrow::Cow;

use bookie_core::Value;
use thiserror::Error;

use crate::text_match::{TextMatchError, TextMatchOptions, TextMatcher};
use crate::value_exact::exact_equal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnScope {
    Any,
    Column(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResultPredicate {
    Contains {
        scope: ColumnScope,
        needle: String,
        options: TextMatchOptions,
    },
    Equals {
        column: usize,
        value: Value,
    },
    NotEquals {
        column: usize,
        value: Value,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PredicateError {
    #[error("the search text is empty")]
    EmptyNeedle,
    #[error("the column is not in the result")]
    UnknownColumn,
    #[error("the value cannot be compared exactly")]
    UncomparableValue,
    #[error("the regular expression is invalid")]
    InvalidPattern,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CellTally {
    pub unsearchable_cells: u64,
}

#[derive(Clone, Debug)]
pub struct PreparedPredicate {
    kind: PreparedKind,
}

#[derive(Clone, Debug)]
enum PreparedKind {
    Contains { scope: ColumnScope, matcher: TextMatcher },
    Equals { column: usize, value: Value },
    NotEquals { column: usize, value: Value },
}

impl ResultPredicate {
    pub fn prepare(self, column_count: usize) -> Result<PreparedPredicate, PredicateError> {
        let kind = match self {
            Self::Contains { scope, needle, options } => prepare_contains(scope, needle, options, column_count)?,
            Self::Equals { column, value } => PreparedKind::Equals {
                column: checked_column(column, column_count)?,
                value: comparable_value(value)?,
            },
            Self::NotEquals { column, value } => PreparedKind::NotEquals {
                column: checked_column(column, column_count)?,
                value: comparable_value(value)?,
            },
        };
        Ok(PreparedPredicate { kind })
    }
}

fn prepare_contains(
    scope: ColumnScope,
    needle: String,
    options: TextMatchOptions,
    column_count: usize,
) -> Result<PreparedKind, PredicateError> {
    if let ColumnScope::Column(column) = scope {
        checked_column(column, column_count)?;
    }
    let matcher = TextMatcher::compile(&needle, options)?.ok_or(PredicateError::EmptyNeedle)?;
    Ok(PreparedKind::Contains { scope, matcher })
}

fn checked_column(column: usize, column_count: usize) -> Result<usize, PredicateError> {
    (column < column_count)
        .then_some(column)
        .ok_or(PredicateError::UnknownColumn)
}

fn comparable_value(value: Value) -> Result<Value, PredicateError> {
    if matches!(value, Value::Undecodable(_)) {
        return Err(PredicateError::UncomparableValue);
    }
    Ok(value)
}

impl From<TextMatchError> for PredicateError {
    fn from(error: TextMatchError) -> Self {
        match error {
            TextMatchError::EmptyNeedle => Self::EmptyNeedle,
            TextMatchError::InvalidPattern => Self::InvalidPattern,
        }
    }
}

impl PreparedPredicate {
    pub fn matches(&self, row: &[Value], tally: &mut CellTally) -> bool {
        match &self.kind {
            PreparedKind::Contains { scope, matcher } => contains_in_row(row, scope, matcher, tally),
            PreparedKind::Equals { column, value } => match row.get(*column) {
                Some(cell) if is_unreadable(cell) => {
                    tally.unsearchable_cells = tally.unsearchable_cells.saturating_add(1);
                    false
                }
                Some(cell) => exact_equal(cell, value),
                None => false,
            },
            PreparedKind::NotEquals { column, value } => match row.get(*column) {
                Some(cell) if is_unreadable(cell) => {
                    tally.unsearchable_cells = tally.unsearchable_cells.saturating_add(1);
                    false
                }
                Some(cell) => !exact_equal(cell, value),
                None => true,
            },
        }
    }
}

fn is_unreadable(cell: &Value) -> bool {
    matches!(cell, Value::Undecodable(_)) && !cell.is_missing_field()
}

fn contains_in_row(row: &[Value], scope: &ColumnScope, matcher: &TextMatcher, tally: &mut CellTally) -> bool {
    match scope {
        ColumnScope::Any => row.iter().any(|cell| cell_contains(cell, matcher, tally)),
        ColumnScope::Column(column) => row.get(*column).is_some_and(|cell| cell_contains(cell, matcher, tally)),
    }
}

fn cell_contains(cell: &Value, matcher: &TextMatcher, tally: &mut CellTally) -> bool {
    match search_text(cell) {
        SearchText::Text(text) => matcher.matches(&text),
        SearchText::TextOrHex(text, bytes) => matcher.matches(text) || matcher.matches(&hex_text(bytes)),
        SearchText::Never => false,
        SearchText::Unsearchable => {
            tally.unsearchable_cells = tally.unsearchable_cells.saturating_add(1);
            false
        }
    }
}

enum SearchText<'a> {
    Text(Cow<'a, str>),
    TextOrHex(&'a str, &'a [u8]),
    Never,
    Unsearchable,
}

fn search_text(value: &Value) -> SearchText<'_> {
    match value {
        Value::Null => SearchText::Never,
        Value::Undecodable(_) if value.is_missing_field() => SearchText::Never,
        Value::Undecodable(_) => SearchText::Unsearchable,
        Value::Text(text) | Value::JsonText(text) => SearchText::Text(Cow::Borrowed(text)),
        Value::Bool(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Int(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Float(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Decimal(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Date(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Time(value) => SearchText::Text(Cow::Owned(value.format("%H:%M:%S").to_string())),
        Value::DateTime(value) => SearchText::Text(Cow::Owned(value.format("%Y-%m-%d %H:%M:%S%.f").to_string())),
        Value::TimestampTz(value) => SearchText::Text(Cow::Owned(value.format("%Y-%m-%d %H:%M:%S%:z").to_string())),
        Value::Uuid(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Json(value) => SearchText::Text(Cow::Owned(value.to_string())),
        Value::Bytes(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) if !text.contains('\0') => SearchText::TextOrHex(text, bytes),
            _ => SearchText::Text(Cow::Owned(hex_text(bytes))),
        },
    }
}

fn hex_text(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

#[cfg(test)]
mod tests;
