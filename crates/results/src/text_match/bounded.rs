use super::{Captures, Strategy, TextMatcher, next_ignore_case_span};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("replacement output exceeds its byte limit")]
pub struct ReplacementLimit;

impl TextMatcher {
    pub fn replace_all_bounded(
        &self,
        text: &str,
        replacement: &str,
        limit: usize,
    ) -> Result<(String, usize), ReplacementLimit> {
        let mut out = String::new();
        let mut last = 0;
        let mut count = 0;
        match &self.strategy {
            Strategy::Expression(expression) => {
                for captures in expression.captures_iter(text) {
                    let Some(found) = captures.get(0).filter(|found| !found.is_empty()) else {
                        continue;
                    };
                    append(&mut out, &text[last..found.start()], limit)?;
                    if self.expands_templates {
                        expand(&captures, replacement, &mut out, limit)?;
                    } else {
                        append(&mut out, replacement, limit)?;
                    }
                    last = found.end();
                    count += 1;
                }
            }
            Strategy::Literal { needle, match_case } => {
                while let Some((start, end)) = literal_span(&text[last..], needle, *match_case) {
                    append(&mut out, &text[last..last + start], limit)?;
                    append(&mut out, replacement, limit)?;
                    last += end;
                    count += 1;
                }
            }
        }
        append(&mut out, &text[last..], limit)?;
        Ok((out, count))
    }
}

fn literal_span(text: &str, needle: &str, match_case: bool) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    if match_case {
        return text.find(needle).map(|start| (start, start + needle.len()));
    }
    next_ignore_case_span(text, needle)
}

fn append(out: &mut String, fragment: &str, limit: usize) -> Result<(), ReplacementLimit> {
    if fragment.len() > limit.saturating_sub(out.len()) {
        return Err(ReplacementLimit);
    }
    out.push_str(fragment);
    Ok(())
}

fn expand(captures: &Captures<'_>, mut template: &str, out: &mut String, limit: usize) -> Result<(), ReplacementLimit> {
    while let Some(start) = template.find('$') {
        append(out, &template[..start], limit)?;
        template = &template[start + 1..];
        if let Some(rest) = template.strip_prefix('$') {
            append(out, "$", limit)?;
            template = rest;
            continue;
        }
        let Some((name, consumed)) = reference(template) else {
            append(out, "$", limit)?;
            continue;
        };
        let capture = match name.parse::<usize>() {
            Ok(index) => captures.get(index),
            Err(_) => captures.name(name),
        };
        if let Some(capture) = capture {
            append(out, capture.as_str(), limit)?;
        }
        template = &template[consumed..];
    }
    append(out, template, limit)
}

fn reference(template: &str) -> Option<(&str, usize)> {
    if let Some(rest) = template.strip_prefix('{') {
        let end = rest.find('}')?;
        return Some((&rest[..end], end + 2));
    }
    let end = template
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        .count();
    (end > 0).then_some((&template[..end], end))
}
