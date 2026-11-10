use super::BrowseTabInput;
use crate::ui::grid::GridMsg;

pub(super) fn to_input(msg: GridMsg) -> BrowseTabInput {
    match msg {
        GridMsg::EditsFlushed { revision, reply } => BrowseTabInput::EditsFlushed { revision, reply },
        GridMsg::SortChanged(keys) => BrowseTabInput::SortChanged { keys },
        GridMsg::CellEdited {
            row_position,
            col_index,
            new_value,
            row_key,
        } => cell_edited_input(row_position, col_index, new_value, None, row_key),
        GridMsg::CellEditedValue {
            row_position,
            col_index,
            value,
            row_key,
        } => cell_edited_input(row_position, col_index, String::new(), Some(value), row_key),
        GridMsg::ReplaceBatch {
            report,
            generation,
            cancel,
            reply,
        } => replace_batch_input(report, generation, cancel, reply),
        GridMsg::CopyToClipboard(text) => BrowseTabInput::GridCopyToClipboard(text),
        GridMsg::FetchCellValue {
            col_index,
            column_name,
            row_key,
            edit_row_position,
        } => fetch_cell_input(col_index, column_name, row_key, edit_row_position),
        GridMsg::IncompleteRowData => BrowseTabInput::IncompleteRowData,
        GridMsg::ProjectionFailure => BrowseTabInput::ProjectionFailure,
        GridMsg::ShowRowAsJson(text) => BrowseTabInput::GridShowRowAsJson(text),
        GridMsg::ExportResults(result) => BrowseTabInput::GridExportResults(result),
        GridMsg::ExportStreamedResults(request) => BrowseTabInput::GridExportStreamedResults(request),
        GridMsg::CopyRowAsStatement(rows, statement) => BrowseTabInput::GridCopyRowAsStatement(rows, statement),
        GridMsg::SetCellNull {
            row_position,
            col_index,
            row_key,
        } => set_null_input(row_position, col_index, row_key),
        GridMsg::DeleteRowAt {
            row_position,
            row_key,
            draft_id,
        } => BrowseTabInput::GridDeleteRowAt {
            row_position,
            row_key,
            draft_id,
        },
        GridMsg::InsertRow => BrowseTabInput::InsertRow,
        GridMsg::DuplicateRow { row_position } => BrowseTabInput::DuplicateRow { row_position },
        GridMsg::FilterByValue { column, value, exclude } => BrowseTabInput::FilterByValue { column, value, exclude },
        GridMsg::FilterWholeResult { .. } => {
            BrowseTabInput::ShowToast(crate::tr!("Whole-result filters apply to query results"))
        }
        GridMsg::ShowToast(message) => BrowseTabInput::ShowToast(message),
        GridMsg::GoToReference { column, row } => BrowseTabInput::GoToReference { column, row },
        GridMsg::ShowReferencingRows { row } => BrowseTabInput::ShowReferencingRows { row },
        GridMsg::HighlightRulesChanged => BrowseTabInput::HighlightRulesChanged,
    }
}

fn set_null_input(row_position: u32, col_index: usize, row_key: Vec<bookie_core::Value>) -> BrowseTabInput {
    BrowseTabInput::GridSetCellNull {
        row_position,
        col_index,
        row_key,
    }
}

fn replace_batch_input(
    report: bookie_app_services::grid_replace::ReplaceReport,
    generation: u64,
    cancel: tokio_util::sync::CancellationToken,
    reply: tokio::sync::oneshot::Sender<
        Result<
            bookie_app_services::grid_replace::ReplaceAcknowledgement,
            bookie_app_services::grid_replace::ReplaceApplyError,
        >,
    >,
) -> BrowseTabInput {
    BrowseTabInput::GridReplaceBatch {
        report,
        generation,
        cancel,
        reply,
    }
}

fn cell_edited_input(
    row_position: u32,
    col_index: usize,
    new_value: String,
    value: Option<bookie_core::Value>,
    row_key: Vec<bookie_core::Value>,
) -> BrowseTabInput {
    BrowseTabInput::GridCellEdited {
        row_position,
        col_index,
        new_value,
        value,
        row_key,
    }
}

fn fetch_cell_input(
    col_index: usize,
    column_name: String,
    row_key: Vec<bookie_core::Value>,
    edit_row_position: Option<u32>,
) -> BrowseTabInput {
    BrowseTabInput::GridFetchCellValue {
        col_index,
        column_name,
        row_key,
        edit_row_position,
    }
}
