use bookie_app_services::text_match::{TextMatchError, TextMatcher};
use bookie_core::Value;
use tokio_util::sync::CancellationToken;

use super::query::FindQuery;

#[derive(Debug, thiserror::Error)]
pub(super) enum ReplaceError {
    #[error(transparent)]
    Match(#[from] TextMatchError),
    #[error("replacement was cancelled")]
    Cancelled,
    #[error("replacement output exceeds its budget")]
    Budget,
}

pub(super) struct ReplaceEdit {
    pub position: u32,
    pub column: usize,
    pub new_value: String,
    pub row_key: Vec<Value>,
}

pub(super) struct ReplaceReport {
    pub edits: Vec<ReplaceEdit>,
    pub replacements: usize,
    pub skipped: usize,
}

pub(super) fn plan_replacements(
    count: u32,
    mut visit_row: impl FnMut(u32, &mut dyn FnMut(usize, &Value)),
    mut row_key_at: impl FnMut(u32) -> Option<Vec<Value>>,
    query: &FindQuery,
    replacement: &str,
    editable: &[bool],
    cancel: &CancellationToken,
) -> Result<ReplaceReport, ReplaceError> {
    if cancel.is_cancelled() {
        return Err(ReplaceError::Cancelled);
    }
    let Some(matcher) = TextMatcher::compile(&query.needle, query.options)? else {
        return Ok(ReplaceReport {
            edits: Vec::new(),
            replacements: 0,
            skipped: 0,
        });
    };
    let mut output_bytes = 0usize;
    let mut over_budget = false;
    let mut report = ReplaceReport {
        edits: Vec::new(),
        replacements: 0,
        skipped: 0,
    };
    for position in 0..count {
        if cancel.is_cancelled() {
            return Err(ReplaceError::Cancelled);
        }
        let Some(row_key) = row_key_at(position) else {
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

fn replace_cell(value: &Value, editable: bool, matcher: &TextMatcher, replacement: &str) -> CellReplace {
    if matches!(value, Value::Undecodable(_)) {
        return CellReplace::Skip;
    }
    let (Value::Text(original) | Value::JsonText(original)) = value else {
        if matcher.matches(&super::super::display::value_to_search_text(value)) {
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
        if text.len() > (16 * 1024 * 1024usize).saturating_sub(*output_bytes) || self.edits.len() >= 10_000 {
            return Err(ReplaceError::Budget);
        }
        *output_bytes += text.len();
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
