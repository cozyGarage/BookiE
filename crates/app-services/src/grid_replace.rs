use crate::text_match::{TextMatchError, TextMatcher};
use bookie_core::Value;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

pub struct ReplaceRequest<'a> {
    pub query: &'a ReplaceQuery,
    pub replacement: &'a str,
    pub editable: &'a [bool],
}

pub struct ReplaceQuery {
    pub needle: String,
    pub options: crate::text_match::TextMatchOptions,
    pub column: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReplaceError {
    #[error(transparent)]
    Match(#[from] TextMatchError),
    #[error("replacement was cancelled")]
    Cancelled,
    #[error("replacement output exceeds its budget")]
    Budget,
}

pub struct ReplaceEdit {
    pub position: u32,
    pub column: usize,
    pub new_value: String,
    pub row_key: Vec<Value>,
}

#[derive(Default)]
pub struct ReplaceReport {
    pub edits: Vec<ReplaceEdit>,
    pub replacements: usize,
    pub skipped: usize,
}

pub fn plan_replacements(
    count: u32,
    mut visit_row: impl FnMut(u32, &mut dyn FnMut(usize, &Value)),
    mut row_key_at: impl FnMut(u32) -> Option<Vec<Value>>,
    request: ReplaceRequest<'_>,
    cancel: &CancellationToken,
    search_text: impl Fn(&Value) -> String,
) -> Result<ReplaceReport, ReplaceError> {
    let ReplaceRequest {
        query,
        replacement,
        editable,
    } = request;
    if cancel.is_cancelled() {
        return Err(ReplaceError::Cancelled);
    }
    let Some(matcher) = TextMatcher::compile(&query.needle, query.options)? else {
        return Ok(ReplaceReport::default());
    };
    let mut output_bytes = 0usize;
    let mut over_budget = false;
    let mut report = ReplaceReport::default();
    for position in 0..count {
        if cancel.is_cancelled() {
            return Err(ReplaceError::Cancelled);
        }
        let Some(row_key) = row_key_at(position).filter(|key| !crate::change_tracker::pk_values_are_unreadable(key))
        else {
            report.skipped = report.skipped.saturating_add(1);
            continue;
        };
        visit_row(position, &mut |column, value| {
            if over_budget || cancel.is_cancelled() {
                return;
            }
            if query.column.is_some_and(|wanted| wanted != column) {
                return;
            }
            match replace_cell(
                value,
                editable.get(column).copied().unwrap_or(false),
                &matcher,
                replacement,
                &search_text,
            ) {
                CellReplace::Budget => over_budget = true,
                CellReplace::Skip => report.skipped = report.skipped.saturating_add(1),
                CellReplace::None => {}
                CellReplace::Edit { text, count } => {
                    if report
                        .push_edit(position, column, text, count, &row_key, &mut output_bytes)
                        .is_err()
                    {
                        over_budget = true;
                    }
                }
            }
        });
        if over_budget {
            return Err(ReplaceError::Budget);
        }
    }
    if cancel.is_cancelled() {
        return Err(ReplaceError::Cancelled);
    }
    Ok(report)
}

enum CellReplace {
    Budget,
    Skip,
    None,
    Edit { text: String, count: usize },
}

fn replace_cell(
    value: &Value,
    editable: bool,
    matcher: &TextMatcher,
    replacement: &str,
    search_text: &impl Fn(&Value) -> String,
) -> CellReplace {
    if matches!(value, Value::Undecodable(_)) {
        return CellReplace::Skip;
    }
    let (Value::Text(original) | Value::JsonText(original)) = value else {
        if matcher.matches(&search_text(value)) {
            return CellReplace::Skip;
        }
        return CellReplace::None;
    };
    if !editable {
        return if matcher.matches(original) {
            CellReplace::Skip
        } else {
            CellReplace::None
        };
    }
    if !matcher.matches(original) {
        return CellReplace::None;
    }
    let Ok((text, count)) = matcher.replace_all_bounded(original, replacement, 1024 * 1024) else {
        return CellReplace::Budget;
    };
    if count == 0 {
        CellReplace::None
    } else {
        CellReplace::Edit { text, count }
    }
}

impl ReplaceReport {
    fn push_edit(
        &mut self,
        position: u32,
        column: usize,
        text: String,
        count: usize,
        row_key: &[Value],
        output_bytes: &mut usize,
    ) -> Result<(), ReplaceError> {
        let retained = retained_key_bytes(row_key)
            .saturating_add(text.len())
            .saturating_add(std::mem::size_of::<ReplaceEdit>());
        if retained > (16 * 1024 * 1024usize).saturating_sub(*output_bytes) || self.edits.len() >= 10_000 {
            return Err(ReplaceError::Budget);
        }
        *output_bytes += retained;
        self.replacements = self.replacements.saturating_add(count);
        self.edits.push(ReplaceEdit {
            position,
            column,
            new_value: text,
            row_key: row_key.to_vec(),
        });
        Ok(())
    }
}

fn retained_key_bytes(key: &[Value]) -> usize {
    key.iter().fold(0usize, |total, value| {
        let heap = match value {
            Value::Text(text) | Value::JsonText(text) | Value::Undecodable(text) => text.len(),
            Value::Bytes(bytes) => bytes.len(),
            Value::Json(value) => json_bytes(value),
            _ => 0,
        };
        total.saturating_add(heap).saturating_add(std::mem::size_of::<Value>())
    })
}

fn json_bytes(value: &serde_json::Value) -> usize {
    let heap = match value {
        serde_json::Value::String(text) => text.len(),
        serde_json::Value::Array(values) => values
            .iter()
            .fold(0usize, |sum, value| sum.saturating_add(json_bytes(value))),
        serde_json::Value::Object(values) => values.iter().fold(0usize, |sum, (key, value)| {
            sum.saturating_add(key.len()).saturating_add(json_bytes(value))
        }),
        _ => 0,
    };
    heap.saturating_add(std::mem::size_of::<serde_json::Value>())
}
