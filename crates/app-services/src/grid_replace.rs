use crate::text_match::{TextMatchError, TextMatcher};
use bookie_core::Value;
use tokio_util::sync::CancellationToken;

mod snapshot;
#[cfg(test)]
mod tests;
pub use snapshot::{ReplaceSnapshot, SnapshotRow};

pub const MAX_REPLACE_INPUT_BYTES: usize = 16 * 1024;
pub const MAX_REPLACE_CELL_OUTPUT_BYTES: usize = 1024 * 1024;
pub const MAX_REPLACE_PLAN_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_REPLACE_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_REPLACE_EDITS: usize = 10_000;
pub const MAX_REPLACE_SNAPSHOT_ROWS: usize = bookie_core::MAX_QUERY_ROWS;

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
    pub original: Value,
    pub new_value: String,
    pub row_key: Vec<Value>,
}

#[derive(Default)]
pub struct ReplaceReport {
    pub edits: Vec<ReplaceEdit>,
    pub replacements: usize,
    pub skipped: usize,
    retained_bytes: usize,
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
    validate_input(query, replacement)?;
    check_cancelled(cancel)?;
    let Some(matcher) = TextMatcher::compile(&query.needle, query.options)? else {
        return Ok(ReplaceReport::default());
    };
    let mut over_budget = false;
    let mut report = ReplaceReport::default();
    for position in 0..count {
        check_cancelled(cancel)?;
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
                cancel,
            ) {
                CellReplace::Budget | CellReplace::Cancelled => over_budget = true,
                CellReplace::Skip => report.skipped = report.skipped.saturating_add(1),
                CellReplace::None => {}
                CellReplace::Edit { text, count } => {
                    if report
                        .push_edit(position, column, value, text, count, &row_key)
                        .is_err()
                    {
                        over_budget = true;
                    }
                }
            }
        });
        check_cancelled(cancel)?;
        if over_budget {
            return Err(ReplaceError::Budget);
        }
    }
    check_cancelled(cancel)?;
    Ok(report)
}

pub fn plan_snapshot_replacements(
    snapshot: &ReplaceSnapshot,
    request: ReplaceRequest<'_>,
    cancel: &CancellationToken,
    search_text: impl Fn(&Value) -> String,
) -> Result<ReplaceReport, ReplaceError> {
    validate_input(request.query, request.replacement)?;
    check_cancelled(cancel)?;
    let Some(matcher) = TextMatcher::compile(&request.query.needle, request.query.options)? else {
        return Ok(ReplaceReport::default());
    };
    let mut report = ReplaceReport::default();
    for row in snapshot.rows() {
        check_cancelled(cancel)?;
        if row.key.is_empty() || crate::change_tracker::pk_values_are_unreadable(&row.key) {
            report.skipped = report.skipped.saturating_add(1);
            continue;
        }
        for (column, value) in &row.cells {
            check_cancelled(cancel)?;
            if request.query.column.is_some_and(|wanted| wanted != *column) {
                continue;
            }
            match replace_cell(
                value,
                request.editable.get(*column).copied().unwrap_or(false),
                &matcher,
                request.replacement,
                &search_text,
                cancel,
            ) {
                CellReplace::Cancelled => return Err(ReplaceError::Cancelled),
                CellReplace::Budget => return Err(ReplaceError::Budget),
                CellReplace::Skip => report.skipped = report.skipped.saturating_add(1),
                CellReplace::None => {}
                CellReplace::Edit { text, count } => {
                    report.push_edit(row.position, *column, value, text, count, &row.key)?
                }
            }
        }
    }
    check_cancelled(cancel)?;
    Ok(report)
}

enum CellReplace {
    Budget,
    Cancelled,
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
    cancel: &CancellationToken,
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
    let Ok((text, count)) =
        matcher.replace_all_bounded_controlled(original, replacement, MAX_REPLACE_CELL_OUTPUT_BYTES, || {
            cancel.is_cancelled()
        })
    else {
        return if cancel.is_cancelled() {
            CellReplace::Cancelled
        } else {
            CellReplace::Budget
        };
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
        original: &Value,
        text: String,
        count: usize,
        row_key: &[Value],
    ) -> Result<(), ReplaceError> {
        let retained = retained_key_bytes(row_key)
            .saturating_add(retained_key_bytes(std::slice::from_ref(original)))
            .saturating_add(text.len())
            .saturating_add(std::mem::size_of::<ReplaceEdit>());
        if retained > MAX_REPLACE_PLAN_BYTES.saturating_sub(self.retained_bytes)
            || self.edits.len() >= MAX_REPLACE_EDITS
        {
            return Err(ReplaceError::Budget);
        }
        self.retained_bytes += retained;
        self.replacements = self.replacements.saturating_add(count);
        self.edits.push(ReplaceEdit {
            position,
            column,
            original: original.clone(),
            new_value: text,
            row_key: row_key.to_vec(),
        });
        Ok(())
    }
}

fn validate_input(query: &ReplaceQuery, replacement: &str) -> Result<(), ReplaceError> {
    if query.needle.len() > MAX_REPLACE_INPUT_BYTES || replacement.len() > MAX_REPLACE_INPUT_BYTES {
        return Err(ReplaceError::Budget);
    }
    Ok(())
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

fn check_cancelled(cancel: &CancellationToken) -> Result<(), ReplaceError> {
    if cancel.is_cancelled() {
        return Err(ReplaceError::Cancelled);
    }
    Ok(())
}
