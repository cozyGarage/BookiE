use std::cmp::Ordering;

use bookie_core::FilterValue;
use regex::{Regex, RegexBuilder};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const HIGHLIGHT_COLORS: &[&str] = &["blue", "teal", "green", "yellow", "orange", "red", "purple", "brown"];
const MAX_LIKE_PATTERN_BYTES: usize = 4096;

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

#[derive(Debug, Clone)]
pub struct PreparedHighlightRule {
    column: usize,
    op: HighlightOp,
    value: Option<FilterValue>,
    like_pattern: Option<Regex>,
    color: &'static str,
    target: HighlightTarget,
}

impl PartialEq for PreparedHighlightRule {
    fn eq(&self, other: &Self) -> bool {
        self.column == other.column
            && self.op == other.op
            && self.value == other.value
            && self.color == other.color
            && self.target == other.target
    }
}

impl Eq for PreparedHighlightRule {}

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
    #[error("the highlight LIKE pattern exceeds the supported size")]
    PatternTooLong,
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
        let like_pattern = compile_like_pattern(rule.op, rule.value.as_ref())?;
        prepared.push(PreparedHighlightRule {
            column,
            op: rule.op,
            value: rule.value.clone(),
            like_pattern,
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
        if display_matches(rule, cell) {
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
            if display_matches(rule, cell) {
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
            rule,
            DisplayCell {
                text: text.as_str(),
                is_null,
            },
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

fn compile_like_pattern(op: HighlightOp, value: Option<&FilterValue>) -> Result<Option<Regex>, HighlightError> {
    if !matches!(op, HighlightOp::Like | HighlightOp::NotLike | HighlightOp::Ilike) {
        return Ok(None);
    }
    let Some(pattern) = single(value) else {
        return Err(HighlightError::InvalidValue);
    };
    if pattern.len() > MAX_LIKE_PATTERN_BYTES {
        return Err(HighlightError::PatternTooLong);
    }
    let pattern = if op == HighlightOp::Ilike {
        pattern.to_lowercase()
    } else {
        pattern.to_string()
    };
    let mut expression = String::with_capacity(pattern.len().saturating_mul(6).saturating_add(8));
    expression.push_str(r"\A");
    let mut literal_start = 0;
    for (index, character) in pattern.char_indices() {
        let wildcard = match character {
            '%' => Some("(?s:.*)"),
            '_' => Some("(?s:.)"),
            _ => None,
        };
        if let Some(wildcard) = wildcard {
            expression.push_str(&regex::escape(&pattern[literal_start..index]));
            expression.push_str(wildcard);
            literal_start = index + character.len_utf8();
        }
    }
    expression.push_str(&regex::escape(&pattern[literal_start..]));
    expression.push_str(r"\z");
    RegexBuilder::new(&expression)
        .size_limit(1 << 20)
        .build()
        .map(Some)
        .map_err(|_| HighlightError::InvalidValue)
}

fn display_matches(rule: &PreparedHighlightRule, cell: DisplayCell<'_>) -> bool {
    match rule.op {
        HighlightOp::IsNull => cell.is_null,
        HighlightOp::IsNotNull => !cell.is_null,
        HighlightOp::IsEmpty => !cell.is_null && cell.text.is_empty(),
        HighlightOp::IsNotEmpty => !cell.is_null && !cell.text.is_empty(),
        HighlightOp::Eq => single(rule.value.as_ref()).is_some_and(|needle| cell.text == needle),
        HighlightOp::NotEq => single(rule.value.as_ref()).is_some_and(|needle| cell.text != needle),
        HighlightOp::Contains => {
            single(rule.value.as_ref()).is_some_and(|needle| contains_ignore_case(cell.text, needle))
        }
        HighlightOp::StartsWith => {
            single(rule.value.as_ref()).is_some_and(|needle| starts_ignore_case(cell.text, needle))
        }
        HighlightOp::EndsWith => single(rule.value.as_ref()).is_some_and(|needle| ends_ignore_case(cell.text, needle)),
        HighlightOp::Like => rule
            .like_pattern
            .as_ref()
            .is_some_and(|pattern| pattern.is_match(cell.text)),
        HighlightOp::NotLike => rule
            .like_pattern
            .as_ref()
            .is_some_and(|pattern| !pattern.is_match(cell.text)),
        HighlightOp::Ilike => rule
            .like_pattern
            .as_ref()
            .is_some_and(|pattern| pattern.is_match(&cell.text.to_lowercase())),
        HighlightOp::In => list(rule.value.as_ref()).is_some_and(|items| items.iter().any(|item| item == cell.text)),
        HighlightOp::NotIn => list(rule.value.as_ref()).is_some_and(|items| items.iter().all(|item| item != cell.text)),
        HighlightOp::Between => pair(rule.value.as_ref()).is_some_and(|(lo, hi)| {
            ordered(cell.text, lo) != Ordering::Less && ordered(cell.text, hi) != Ordering::Greater
        }),
        HighlightOp::Lt => {
            single(rule.value.as_ref()).is_some_and(|needle| ordered(cell.text, needle) == Ordering::Less)
        }
        HighlightOp::LtEq => {
            single(rule.value.as_ref()).is_some_and(|needle| ordered(cell.text, needle) != Ordering::Greater)
        }
        HighlightOp::Gt => {
            single(rule.value.as_ref()).is_some_and(|needle| ordered(cell.text, needle) == Ordering::Greater)
        }
        HighlightOp::GtEq => {
            single(rule.value.as_ref()).is_some_and(|needle| ordered(cell.text, needle) != Ordering::Less)
        }
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

#[cfg(test)]
mod tests;
