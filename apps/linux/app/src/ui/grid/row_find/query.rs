use bookie_app_services::text_match::{TextMatchOptions, TextMatcher};
use bookie_core::Value;

use super::super::display::{value_to_display_text, value_to_search_text};
use super::json_search::{CaseInsensitivePattern, matches_value};

pub(super) const MAX_MATCHES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FindQuery {
    pub needle: String,
    pub options: TextMatchOptions,
    pub column: Option<usize>,
}

impl FindQuery {
    pub(super) fn new(needle: impl Into<String>, options: TextMatchOptions, column: Option<usize>) -> Self {
        Self {
            needle: needle.into(),
            options,
            column,
        }
    }

    pub(super) fn uses_default_literal(&self) -> bool {
        !self.options.match_case && !self.options.whole_words && !self.options.regex
    }
}

pub(super) struct RowMatch {
    pub position: u32,
    pub column: usize,
    pub text: String,
}

pub(super) fn matching_rows(
    count: u32,
    mut visit_row: impl FnMut(u32, &mut dyn FnMut(usize, &Value) -> bool),
    query: &FindQuery,
    limit: usize,
) -> Result<Vec<RowMatch>, bookie_app_services::text_match::TextMatchError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    if query.uses_default_literal() {
        return Ok(match_default_literal(count, &mut visit_row, query, limit));
    }
    let Some(matcher) = TextMatcher::compile(&query.needle, query.options)? else {
        return Ok(Vec::new());
    };
    Ok(match_with_matcher(count, &mut visit_row, query.column, &matcher, limit))
}

fn match_default_literal(
    count: u32,
    visit_row: &mut impl FnMut(u32, &mut dyn FnMut(usize, &Value) -> bool),
    query: &FindQuery,
    limit: usize,
) -> Vec<RowMatch> {
    let needle = query.needle.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let needle = CaseInsensitivePattern::new(&needle);
    let mut found = Vec::new();
    for position in 0..count {
        let mut matching_cell = None;
        visit_row(position, &mut |column, value| {
            if query.column.is_some_and(|wanted| wanted != column) {
                return false;
            }
            if matches_value(value, &needle) {
                matching_cell = Some((column, value_to_display_text(value)));
                true
            } else {
                false
            }
        });
        if let Some((column, text)) = matching_cell {
            found.push(RowMatch { position, column, text });
            if found.len() == limit {
                break;
            }
        }
    }
    found
}

fn match_with_matcher(
    count: u32,
    visit_row: &mut impl FnMut(u32, &mut dyn FnMut(usize, &Value) -> bool),
    column: Option<usize>,
    matcher: &TextMatcher,
    limit: usize,
) -> Vec<RowMatch> {
    let mut found = Vec::new();
    for position in 0..count {
        let mut matching_cell = None;
        visit_row(position, &mut |index, value| {
            if column.is_some_and(|wanted| wanted != index) {
                return false;
            }
            if matcher.matches(&value_to_search_text(value)) {
                matching_cell = Some((index, value_to_display_text(value)));
                true
            } else {
                false
            }
        });
        if let Some((column, text)) = matching_cell {
            found.push(RowMatch { position, column, text });
            if found.len() == limit {
                break;
            }
        }
    }
    found
}
