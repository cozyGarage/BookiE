use bookie_app_services::text_match::{TextMatchError, TextMatcher};
use bookie_core::Value;
use tokio_util::sync::CancellationToken;

use super::query::FindQuery;

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
) -> Result<ReplaceReport, TextMatchError> {
    let Some(matcher) = TextMatcher::compile(&query.needle, query.options)? else {
        return Ok(ReplaceReport {
            edits: Vec::new(),
            replacements: 0,
            skipped: 0,
        });
    };
    let mut report = ReplaceReport {
        edits: Vec::new(),
        replacements: 0,
        skipped: 0,
    };
    for position in 0..count {
        if cancel.is_cancelled() {
            break;
        }
        let Some(row_key) = row_key_at(position) else {
            report.skipped = report.skipped.saturating_add(1);
            continue;
        };
        visit_row(position, &mut |column, value| {
            if query.column.is_some_and(|wanted| wanted != column) {
                return;
            }
            match replace_cell(
                value,
                editable.get(column).copied().unwrap_or(false),
                &matcher,
                replacement,
            ) {
                CellReplace::Skip => report.skipped = report.skipped.saturating_add(1),
                CellReplace::None => {}
                CellReplace::Edit { text, count } => {
                    report.replacements = report.replacements.saturating_add(count);
                    report.edits.push(ReplaceEdit {
                        position,
                        column,
                        new_value: text,
                        row_key: row_key.clone(),
                    });
                }
            }
        });
    }
    Ok(report)
}

enum CellReplace {
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
    let (text, count) = matcher.replace_all(original, replacement);
    if count == 0 {
        CellReplace::None
    } else {
        CellReplace::Edit { text, count }
    }
}
