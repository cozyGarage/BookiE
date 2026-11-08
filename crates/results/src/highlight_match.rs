use std::cmp::Ordering;

use bookie_core::FilterValue;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const HIGHLIGHT_COLORS: &[&str] = &["blue", "teal", "green", "yellow", "orange", "red", "purple", "brown"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightTarget {
    Cell,
    Row,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Contains,
    StartsWith,
    EndsWith,
    Like,
    NotLike,
    Ilike,
    IsEmpty,
    IsNotEmpty,
    IsNull,
    IsNotNull,
    In,
    NotIn,
    Between,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightRule {
    pub column: String,
    pub op: HighlightOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<FilterValue>,
    pub color: String,
    pub target: HighlightTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HighlightRuleSet {
    #[serde(default)]
    pub rules: Vec<HighlightRule>,
}

impl HighlightRuleSet {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedHighlightRule {
    column: usize,
    op: HighlightOp,
    value: Option<FilterValue>,
    color: &'static str,
    target: HighlightTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayCell<'a> {
    pub text: &'a str,
    pub is_null: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightPaint {
    pub color: &'static str,
    pub target: HighlightTarget,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HighlightError {
    #[error("the colour is not in the highlight palette")]
    UnknownColor,
    #[error("the highlight rule value shape does not match the operator")]
    InvalidValue,
}

pub fn highlight_color(name: &str) -> Option<&'static str> {
    let trimmed = name.trim().to_ascii_lowercase();
    HIGHLIGHT_COLORS.iter().copied().find(|known| *known == trimmed)
}

pub fn prepare_rules(rules: &[HighlightRule], columns: &[&str]) -> Result<Vec<PreparedHighlightRule>, HighlightError> {
    let mut prepared = Vec::with_capacity(rules.len());
    for rule in rules {
        let Some(column) = columns.iter().position(|name| *name == rule.column) else {
            continue;
        };
        let color = highlight_color(&rule.color).ok_or(HighlightError::UnknownColor)?;
        validate_value(rule.op, rule.value.as_ref())?;
        prepared.push(PreparedHighlightRule {
            column,
            op: rule.op,
            value: rule.value.clone(),
            color,
            target: rule.target,
        });
    }
    Ok(prepared)
}

pub fn paint_for_cell(
    rules: &[PreparedHighlightRule],
    column: usize,
    cell: DisplayCell<'_>,
    mut row_cell: impl FnMut(usize) -> Option<(String, bool)>,
) -> Option<HighlightPaint> {
    for rule in rules {
        if rule.target != HighlightTarget::Cell || rule.column != column {
            continue;
        }
        if display_matches(rule.op, cell, rule.value.as_ref()) {
            return Some(HighlightPaint {
                color: rule.color,
                target: HighlightTarget::Cell,
            });
        }
    }
    for rule in rules {
        if rule.target != HighlightTarget::Row {
            continue;
        }
        if rule.column == column {
            if display_matches(rule.op, cell, rule.value.as_ref()) {
                return Some(HighlightPaint {
                    color: rule.color,
                    target: HighlightTarget::Row,
                });
            }
            continue;
        }
        let Some((text, is_null)) = row_cell(rule.column) else {
            continue;
        };
        if display_matches(
            rule.op,
            DisplayCell {
                text: text.as_str(),
                is_null,
            },
            rule.value.as_ref(),
        ) {
            return Some(HighlightPaint {
                color: rule.color,
                target: HighlightTarget::Row,
            });
        }
    }
    None
}

fn validate_value(op: HighlightOp, value: Option<&FilterValue>) -> Result<(), HighlightError> {
    let ok = match op {
        HighlightOp::IsEmpty | HighlightOp::IsNotEmpty | HighlightOp::IsNull | HighlightOp::IsNotNull => {
            value.is_none()
        }
        HighlightOp::Between => matches!(value, Some(FilterValue::Pair(_, _))),
        HighlightOp::In | HighlightOp::NotIn => matches!(value, Some(FilterValue::List(_))),
        _ => matches!(value, Some(FilterValue::Single(_))),
    };
    ok.then_some(()).ok_or(HighlightError::InvalidValue)
}

fn display_matches(op: HighlightOp, cell: DisplayCell<'_>, value: Option<&FilterValue>) -> bool {
    match op {
        HighlightOp::IsNull => cell.is_null,
        HighlightOp::IsNotNull => !cell.is_null,
        HighlightOp::IsEmpty => !cell.is_null && cell.text.is_empty(),
        HighlightOp::IsNotEmpty => !cell.is_null && !cell.text.is_empty(),
        HighlightOp::Eq => single(value).is_some_and(|needle| cell.text == needle),
        HighlightOp::NotEq => single(value).is_some_and(|needle| cell.text != needle),
        HighlightOp::Contains => single(value).is_some_and(|needle| contains_ignore_case(cell.text, needle)),
        HighlightOp::StartsWith => single(value).is_some_and(|needle| starts_ignore_case(cell.text, needle)),
        HighlightOp::EndsWith => single(value).is_some_and(|needle| ends_ignore_case(cell.text, needle)),
        HighlightOp::Like => single(value).is_some_and(|pattern| like_match(cell.text, pattern, false)),
        HighlightOp::NotLike => single(value).is_some_and(|pattern| !like_match(cell.text, pattern, false)),
        HighlightOp::Ilike => single(value).is_some_and(|pattern| like_match(cell.text, pattern, true)),
        HighlightOp::In => list(value).is_some_and(|items| items.iter().any(|item| item == cell.text)),
        HighlightOp::NotIn => list(value).is_some_and(|items| items.iter().all(|item| item != cell.text)),
        HighlightOp::Between => pair(value).is_some_and(|(lo, hi)| {
            ordered(cell.text, lo) != Ordering::Less && ordered(cell.text, hi) != Ordering::Greater
        }),
        HighlightOp::Lt => single(value).is_some_and(|needle| ordered(cell.text, needle) == Ordering::Less),
        HighlightOp::LtEq => single(value).is_some_and(|needle| ordered(cell.text, needle) != Ordering::Greater),
        HighlightOp::Gt => single(value).is_some_and(|needle| ordered(cell.text, needle) == Ordering::Greater),
        HighlightOp::GtEq => single(value).is_some_and(|needle| ordered(cell.text, needle) != Ordering::Less),
    }
}

fn single(value: Option<&FilterValue>) -> Option<&str> {
    match value {
        Some(FilterValue::Single(text)) => Some(text.as_str()),
        _ => None,
    }
}

fn pair(value: Option<&FilterValue>) -> Option<(&str, &str)> {
    match value {
        Some(FilterValue::Pair(lo, hi)) => Some((lo.as_str(), hi.as_str())),
        _ => None,
    }
}

fn list(value: Option<&FilterValue>) -> Option<&[String]> {
    match value {
        Some(FilterValue::List(items)) => Some(items.as_slice()),
        _ => None,
    }
}

fn ordered(left: &str, right: &str) -> Ordering {
    match (parse_decimal(left), parse_decimal(right)) {
        (Some(a), Some(b)) => a.cmp(&b),
        _ => left.cmp(right),
    }
}

fn parse_decimal(text: &str) -> Option<Decimal> {
    text.parse().ok()
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn starts_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack.len() >= needle.len()
        && haystack
            .get(..needle.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle) || prefix.to_lowercase() == needle.to_lowercase())
}

fn ends_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack.len() >= needle.len()
        && haystack
            .get(haystack.len() - needle.len()..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(needle) || suffix.to_lowercase() == needle.to_lowercase())
}

fn like_match(haystack: &str, pattern: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        like_backtrack(&haystack.to_lowercase(), &pattern.to_lowercase())
    } else {
        like_backtrack(haystack, pattern)
    }
}

fn like_backtrack(haystack: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return haystack.is_empty();
    }
    let mut chars = pattern.chars();
    let Some(first) = chars.next() else {
        return haystack.is_empty();
    };
    let rest = chars.as_str();
    match first {
        '%' => {
            if like_backtrack(haystack, rest) {
                return true;
            }
            let mut rest_hay = haystack.chars();
            while rest_hay.next().is_some() {
                if like_backtrack(rest_hay.as_str(), rest) {
                    return true;
                }
            }
            false
        }
        '_' => {
            let mut rest_hay = haystack.chars();
            rest_hay.next().is_some_and(|_| like_backtrack(rest_hay.as_str(), rest))
        }
        expected => {
            let mut rest_hay = haystack.chars();
            rest_hay
                .next()
                .is_some_and(|got| got == expected && like_backtrack(rest_hay.as_str(), rest))
        }
    }
}

#[cfg(test)]
mod tests;
