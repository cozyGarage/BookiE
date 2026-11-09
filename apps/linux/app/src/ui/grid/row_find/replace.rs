use bookie_core::Value;
use tokio_util::sync::CancellationToken;

use super::query::FindQuery;
pub(super) use bookie_app_services::grid_replace::{ReplaceError, ReplaceReport};

pub(super) fn plan_replacements(
    count: u32,
    visit_row: impl FnMut(u32, &mut dyn FnMut(usize, &Value)),
    row_key_at: impl FnMut(u32) -> Option<Vec<Value>>,
    query: &FindQuery,
    replacement: &str,
    editable: &[bool],
    cancel: &CancellationToken,
) -> Result<ReplaceReport, ReplaceError> {
    let query = bookie_app_services::grid_replace::ReplaceQuery {
        needle: query.needle.clone(),
        options: query.options,
        column: query.column,
    };
    bookie_app_services::grid_replace::plan_replacements(
        count,
        visit_row,
        row_key_at,
        bookie_app_services::grid_replace::ReplaceRequest {
            query: &query,
            replacement,
            editable,
        },
        cancel,
        |value| super::super::display::value_to_search_text(value).into_owned(),
    )
}
