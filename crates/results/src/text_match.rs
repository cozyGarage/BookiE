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
    let mut out = String::with_capacity(text.len());
    let mut count = 0usize;
    let mut rest = text;
    while let Some((start, end)) = next_ignore_case_span(rest, needle_lower) {
        out.push_str(&rest[..start]);
        out.push_str(replacement);
        rest = &rest[end..];
        count += 1;
    }
    if count == 0 {
        return (text.to_owned(), 0);
    }
    out.push_str(rest);
    (out, count)
}

fn next_ignore_case_span(haystack: &str, needle_lower: &str) -> Option<(usize, usize)> {
    find_ignore_case_span(haystack, needle_lower)
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
    let (lower, spans) = lowercase_with_source_spans(haystack);
    let start = lower.find(needle_lower)?;
    let end = start + needle_lower.len();
    let first = spans.iter().find(|span| span.1 > start)?;
    let last = spans.iter().rev().find(|span| span.0 < end)?;
    Some((first.2, last.3))
}

fn lowercase_with_source_spans(text: &str) -> (String, Vec<(usize, usize, usize, usize)>) {
    let mut lower = String::with_capacity(text.len());
    let mut spans = Vec::with_capacity(text.chars().count());
    for (source_start, character) in text.char_indices() {
        let lower_start = lower.len();
        lower.extend(character.to_lowercase());
        spans.push((
            lower_start,
            lower.len(),
            source_start,
            source_start + character.len_utf8(),
        ));
    }
    (lower, spans)
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
