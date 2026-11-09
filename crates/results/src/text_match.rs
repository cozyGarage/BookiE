use regex::{Captures, Regex, RegexBuilder};

mod bounded;
pub use bounded::ReplacementLimit;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextMatchOptions {
    pub match_case: bool,
    pub whole_words: bool,
    pub regex: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TextMatchError {
    #[error("the search text is empty")]
    EmptyNeedle,
    #[error("the regular expression is invalid")]
    InvalidPattern,
}

#[derive(Clone, Debug)]
enum Strategy {
    Literal { needle: String, match_case: bool },
    Expression(Regex),
}

#[derive(Clone, Debug)]
pub struct TextMatcher {
    strategy: Strategy,
    expands_templates: bool,
}

impl TextMatcher {
    pub fn compile(term: &str, options: TextMatchOptions) -> Result<Option<Self>, TextMatchError> {
        if options.regex {
            if term.is_empty() {
                return Ok(None);
            }
            return Ok(Some(Self::compiled(term, options, true)?));
        }
        let needle = term.trim();
        if needle.is_empty() {
            return Ok(None);
        }
        if options.whole_words {
            return Ok(Some(Self::compiled(&regex::escape(needle), options, false)?));
        }
        Ok(Some(Self {
            strategy: Strategy::Literal {
                needle: if options.match_case {
                    needle.to_owned()
                } else {
                    needle.to_lowercase()
                },
                match_case: options.match_case,
            },
            expands_templates: false,
        }))
    }

    fn compiled(pattern: &str, options: TextMatchOptions, expands_templates: bool) -> Result<Self, TextMatchError> {
        let wrapped = if options.whole_words {
            format!(r"\b(?:{pattern})\b")
        } else {
            pattern.to_owned()
        };
        let expression = RegexBuilder::new(&wrapped)
            .case_insensitive(!options.match_case)
            .build()
            .map_err(|_| TextMatchError::InvalidPattern)?;
        Ok(Self {
            strategy: Strategy::Expression(expression),
            expands_templates,
        })
    }

    pub fn matches(&self, text: &str) -> bool {
        match &self.strategy {
            Strategy::Literal { needle, match_case } => {
                if *match_case {
                    text.contains(needle.as_str())
                } else {
                    contains_ignore_case(text, needle)
                }
            }
            Strategy::Expression(expression) => expression.find_iter(text).any(|found| !found.is_empty()),
        }
    }

    pub fn replace_all(&self, text: &str, replacement: &str) -> (String, usize) {
        match &self.strategy {
            Strategy::Literal { needle, match_case } => replace_literal(text, needle, *match_case, replacement),
            Strategy::Expression(expression) => {
                if self.expands_templates {
                    replace_expression(expression, text, replacement)
                } else {
                    replace_expression(expression, text, &escape_replacement(replacement))
                }
            }
        }
    }
}

fn replace_expression(expression: &Regex, text: &str, template: &str) -> (String, usize) {
    let mut count = 0usize;
    let mut last = 0usize;
    let mut out = String::with_capacity(text.len());
    for caps in expression.captures_iter(text) {
        let Some(full) = caps.get(0) else {
            continue;
        };
        if full.is_empty() {
            continue;
        }
        out.push_str(&text[last..full.start()]);
        out.push_str(&expand_capture(&caps, template));
        last = full.end();
        count += 1;
    }
    if count == 0 {
        return (text.to_owned(), 0);
    }
    out.push_str(&text[last..]);
    (out, count)
}

fn expand_capture(caps: &Captures<'_>, template: &str) -> String {
    let mut dest = String::new();
    caps.expand(template, &mut dest);
    dest
}

fn replace_literal(text: &str, needle: &str, match_case: bool, replacement: &str) -> (String, usize) {
    if needle.is_empty() {
        return (text.to_owned(), 0);
    }
    if match_case {
        let parts: Vec<&str> = text.split(needle).collect();
        if parts.len() == 1 {
            return (text.to_owned(), 0);
        }
        return (parts.join(replacement), parts.len() - 1);
    }
    replace_ignore_case(text, needle, replacement)
}

fn replace_ignore_case(text: &str, needle_lower: &str, replacement: &str) -> (String, usize) {
    let Ok(matches) = FoldedMatchIter::new(text, needle_lower) else {
        return (text.to_owned(), 0);
    };
    let mut out = String::with_capacity(text.len());
    let mut count = 0usize;
    let mut last = 0;
    for (start, end) in matches {
        out.push_str(&text[last..start]);
        out.push_str(replacement);
        last = end;
        count += 1;
    }
    if count == 0 {
        return (text.to_owned(), 0);
    }
    out.push_str(&text[last..]);
    (out, count)
}

fn find_ignore_case_span(haystack: &str, needle_lower: &str) -> Option<(usize, usize)> {
    if needle_lower.is_empty() {
        return None;
    }
    if needle_lower.is_ascii() {
        let needle = needle_lower.as_bytes();
        let direct = haystack
            .as_bytes()
            .windows(needle.len())
            .position(|window| window.eq_ignore_ascii_case(needle))
            .filter(|start| haystack.is_char_boundary(*start))
            .map(|start| (start, start + needle.len()));
        if haystack.is_ascii() || !haystack.contains(FOLDS_TO_ASCII) {
            return direct;
        }
        let folded = find_ignore_case_folded(haystack, needle_lower);
        return earlier_span(direct, folded);
    }
    find_ignore_case_folded(haystack, needle_lower)
}

fn earlier_span(left: Option<(usize, usize)>, right: Option<(usize, usize)>) -> Option<(usize, usize)> {
    match (left, right) {
        (Some(left), Some(right)) if left.0 <= right.0 => Some(left),
        (_, Some(right)) => Some(right),
        (Some(left), None) => Some(left),
        (None, None) => None,
    }
}

fn find_ignore_case_folded(haystack: &str, needle_lower: &str) -> Option<(usize, usize)> {
    FoldedMatchIter::new(haystack, needle_lower).ok()?.next()
}

struct LowercaseChange {
    lower_start: usize,
    lower_end: usize,
    source_start: usize,
    source_end: usize,
    delta_after: isize,
}

enum FoldedSearch {
    Ascii {
        cursor: usize,
    },
    Lowercase {
        text: String,
        changes: Vec<LowercaseChange>,
        cursor: usize,
    },
}

struct FoldedMatchIter<'text, 'needle> {
    source: &'text str,
    needle: &'needle str,
    search: FoldedSearch,
    source_cursor: usize,
}

impl<'text, 'needle> FoldedMatchIter<'text, 'needle> {
    fn new(source: &'text str, needle: &'needle str) -> Result<Self, ReplacementLimit> {
        Self::new_controlled(source, needle, || false)
    }

    pub(super) fn new_controlled(
        source: &'text str,
        needle: &'needle str,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, ReplacementLimit> {
        if cancelled() {
            return Err(ReplacementLimit::Cancelled);
        }
        if needle.is_empty() {
            return Ok(Self {
                source,
                needle,
                search: FoldedSearch::Ascii { cursor: source.len() },
                source_cursor: source.len(),
            });
        }
        if needle.is_ascii() && !source.contains(FOLDS_TO_ASCII) {
            return Ok(Self {
                source,
                needle,
                search: FoldedSearch::Ascii { cursor: 0 },
                source_cursor: 0,
            });
        }
        let (text, changes) = lowercase_with_changes(source, &cancelled)?;
        Ok(Self {
            source,
            needle,
            search: FoldedSearch::Lowercase {
                text,
                changes,
                cursor: 0,
            },
            source_cursor: 0,
        })
    }
}

impl Iterator for FoldedMatchIter<'_, '_> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.search {
            FoldedSearch::Ascii { cursor } => {
                let bytes = self.source.as_bytes();
                let needle = self.needle.as_bytes();
                while needle.len() <= bytes.len().saturating_sub(*cursor) {
                    let start = *cursor;
                    *cursor += 1;
                    if bytes[start..start + needle.len()].eq_ignore_ascii_case(needle) {
                        *cursor = start + needle.len();
                        return Some((start, *cursor));
                    }
                }
                None
            }
            FoldedSearch::Lowercase { text, changes, cursor } => loop {
                let rest = text.get(*cursor..)?;
                let lower_start = *cursor + rest.find(self.needle)?;
                let lower_end = lower_start + self.needle.len();
                *cursor = lower_end;
                let source_start = source_offset(lower_start, changes, false);
                let source_end = source_offset(lower_end, changes, true);
                if source_start < self.source_cursor {
                    continue;
                }
                self.source_cursor = source_end;
                return Some((source_start, source_end));
            },
        }
    }
}

fn lowercase_with_changes(
    source: &str,
    cancelled: &impl Fn() -> bool,
) -> Result<(String, Vec<LowercaseChange>), ReplacementLimit> {
    let mut lowercase = String::with_capacity(source.len());
    let mut changes = Vec::new();
    let mut delta = 0isize;
    for (source_start, character) in source.char_indices() {
        if cancelled() {
            return Err(ReplacementLimit::Cancelled);
        }
        let lower_start = lowercase.len();
        let mut lower_characters = 0;
        lowercase.extend(character.to_lowercase().inspect(|_| lower_characters += 1));
        let lower_end = lowercase.len();
        let source_end = source_start + character.len_utf8();
        if lower_end - lower_start != source_end - source_start || lower_characters != 1 {
            delta += (lower_end - lower_start) as isize - (source_end - source_start) as isize;
            changes.push(LowercaseChange {
                lower_start,
                lower_end,
                source_start,
                source_end,
                delta_after: delta,
            });
        }
    }
    Ok((lowercase, changes))
}

fn source_offset(lower_offset: usize, changes: &[LowercaseChange], end: bool) -> usize {
    let index = changes.partition_point(|change| change.lower_end <= lower_offset);
    if let Some(change) = changes.get(index)
        && lower_offset > change.lower_start
        && lower_offset < change.lower_end
    {
        return if end { change.source_end } else { change.source_start };
    }
    let delta = index.checked_sub(1).map_or(0, |previous| changes[previous].delta_after);
    if delta >= 0 {
        lower_offset.saturating_sub(delta as usize)
    } else {
        lower_offset.saturating_add(delta.unsigned_abs())
    }
}

fn escape_replacement(template: &str) -> String {
    template.replace('\\', "\\\\").replace('$', "$$")
}

pub(crate) fn contains_ignore_case(haystack: &str, needle_lower: &str) -> bool {
    find_ignore_case_span(haystack, needle_lower).is_some()
}

const FOLDS_TO_ASCII: [char; 2] = ['\u{0130}', '\u{212A}'];

#[cfg(test)]
#[path = "text_match/tests.rs"]
mod tests;
