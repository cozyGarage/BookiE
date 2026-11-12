use super::BrowseTabInput;
use crate::ui::grid::GridMsg;

pub(super) fn to_input(msg: GridMsg) -> BrowseTabInput {
    match row_input(msg) {
        Ok(input) => input,
        Err(msg) => view_input(msg),
    }
}

fn view_input(msg: GridMsg) -> BrowseTabInput {
    match msg {
        GridMsg::EditsFlushed { revision, reply } => BrowseTabInput::EditsFlushed { revision, reply },
        GridMsg::SortChanged(keys) => BrowseTabInput::SortChanged { keys },
        GridMsg::CopyToClipboard(text) => BrowseTabInput::GridCopyToClipboard(text),
        GridMsg::IncompleteRowData => BrowseTabInput::IncompleteRowData,
        GridMsg::ProjectionFailure => BrowseTabInput::ProjectionFailure,
        GridMsg::ShowRowAsJson(text) => BrowseTabInput::GridShowRowAsJson(text),
        GridMsg::ExportResults(result) => BrowseTabInput::GridExportResults(result),
        GridMsg::ExportStreamedResults(request) => BrowseTabInput::GridExportStreamedResults(request),
        GridMsg::CopyRowAsStatement(rows, statement) => BrowseTabInput::GridCopyRowAsStatement(rows, statement),
        GridMsg::ResetCell(request) => BrowseTabInput::GridResetCell(request),
        GridMsg::InsertRow => BrowseTabInput::InsertRow,
        GridMsg::FilterByValue { column, value, exclude } => BrowseTabInput::FilterByValue { column, value, exclude },
        GridMsg::FilterWholeResult { .. } => whole_result_unavailable(),
        GridMsg::ShowToast(message) => BrowseTabInput::ShowToast(message),
        GridMsg::GoToReference { column, row } => BrowseTabInput::GoToReference { column, row },
        GridMsg::ShowReferencingRows { row } => BrowseTabInput::ShowReferencingRows { row },
        GridMsg::ColumnSelectionChanged => BrowseTabInput::ColumnSelectionChanged,
        GridMsg::SetCells(request) => BrowseTabInput::GridSetCells(request),
        GridMsg::CellEdited { .. }
        | GridMsg::CellEditedValue { .. }
        | GridMsg::ReplaceBatch { .. }
        | GridMsg::SetCellsConfirmed { .. }
        | GridMsg::FetchCellValue { .. }
        | GridMsg::SetCellNull { .. }
        | GridMsg::DeleteRowAt { .. }
        | GridMsg::DuplicateRow { .. } => BrowseTabInput::IncompleteRowData,
        GridMsg::HighlightRulesChanged => BrowseTabInput::HighlightRulesChanged,
    }
}

fn row_input(msg: GridMsg) -> Result<BrowseTabInput, GridMsg> {
    Ok(match msg {
        GridMsg::CellEdited {
            row_position,
            col_index,
            new_value,
            row_key,
        } => cell_edited_input(row_position, col_index, new_value, None, row_key, None),
        GridMsg::CellEditedValue {
            row_position,
            col_index,
            value,
            row_key,
            draft_id,
        } => cell_edited_input(row_position, col_index, String::new(), Some(value), row_key, draft_id),
        GridMsg::ReplaceBatch {
            report,
            generation,
            cancel,
            reply,
        } => replace_input(report, generation, cancel, reply),
        GridMsg::FetchCellValue {
            col_index,
            column_name,
            row_key,
            edit_row_position,
        } => fetch_cell_input(col_index, column_name, row_key, edit_row_position),
        GridMsg::SetCellNull {
            row_position,
            col_index,
            row_key,
            draft_id,
        } => set_null_input(row_position, col_index, row_key, draft_id),
        GridMsg::DeleteRowAt {
            row_position,
            row_key,
            draft_id,
        } => delete_row_input(row_position, row_key, draft_id),
        GridMsg::DuplicateRow {
            row_position,
            row_key,
            draft_id,
        } => duplicate_row_input(row_position, row_key, draft_id),
        other => return Err(other),
    })
}

fn whole_result_unavailable() -> BrowseTabInput {
    BrowseTabInput::ShowToast(bookie_app_services::tr!("Whole-result filters apply to query results"))
}

fn delete_row_input(row_position: u32, row_key: Vec<bookie_core::Value>, draft_id: Option<u64>) -> BrowseTabInput {
    BrowseTabInput::GridDeleteRowAt {
        row_position,
        row_key,
        draft_id,
    }
}

fn duplicate_row_input(row_position: u32, row_key: Vec<bookie_core::Value>, draft_id: Option<u64>) -> BrowseTabInput {
    BrowseTabInput::GridDuplicateRow {
        row_position,
        row_key,
        draft_id,
    }
}

fn set_null_input(
    row_position: u32,
    col_index: usize,
    row_key: Vec<bookie_core::Value>,
    draft_id: Option<u64>,
) -> BrowseTabInput {
    BrowseTabInput::GridSetCellNull {
        row_position,
        col_index,
        row_key,
        draft_id,
    }
}

fn replace_input(
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
    draft_id: Option<u64>,
) -> BrowseTabInput {
    BrowseTabInput::GridCellEdited {
        row_position,
        col_index,
        new_value,
        value,
        row_key,
        draft_id,
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
