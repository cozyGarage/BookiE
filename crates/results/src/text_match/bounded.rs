use super::{Captures, FoldedMatchIter, Strategy, TextMatcher};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReplacementLimit {
    #[error("replacement output exceeds its byte limit")]
    Budget,
    #[error("replacement was cancelled")]
    Cancelled,
}

impl TextMatcher {
    pub fn replace_all_bounded(
        &self,
        text: &str,
        replacement: &str,
        limit: usize,
    ) -> Result<(String, usize), ReplacementLimit> {
        self.replace_all_bounded_controlled(text, replacement, limit, || false)
    }

    pub fn replace_all_bounded_controlled(
        &self,
        text: &str,
        replacement: &str,
        limit: usize,
        cancelled: impl Fn() -> bool,
    ) -> Result<(String, usize), ReplacementLimit> {
        if cancelled() {
            return Err(ReplacementLimit::Cancelled);
        }
        let mut out = String::new();
        let mut last = 0;
        let mut count = 0;
        match &self.strategy {
            Strategy::Expression(expression) => {
                for captures in expression.captures_iter(text) {
                    if cancelled() {
                        return Err(ReplacementLimit::Cancelled);
                    }
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
            Strategy::Literal {
                needle,
                match_case: true,
            } => {
                while let Some(start) = text[last..].find(needle) {
                    if cancelled() {
                        return Err(ReplacementLimit::Cancelled);
                    }
                    append(&mut out, &text[last..last + start], limit)?;
                    append(&mut out, replacement, limit)?;
                    last += start + needle.len();
                    count += 1;
                }
            }
            Strategy::Literal {
                needle,
                match_case: false,
            } => {
                let mut matches = FoldedMatchIter::new_controlled(text, needle, &cancelled)?;
                for (start, end) in &mut matches {
                    if cancelled() {
                        return Err(ReplacementLimit::Cancelled);
                    }
                    append(&mut out, &text[last..start], limit)?;
                    append(&mut out, replacement, limit)?;
                    last = end;
                    count += 1;
                }
            }
        }
        if cancelled() {
            return Err(ReplacementLimit::Cancelled);
        }
        append(&mut out, &text[last..], limit)?;
        Ok((out, count))
    }
}

fn append(out: &mut String, fragment: &str, limit: usize) -> Result<(), ReplacementLimit> {
    if fragment.len() > limit.saturating_sub(out.len()) {
        return Err(ReplacementLimit::Budget);
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
