use super::*;

pub(super) fn cell_row_identity(widget: &gtk::Widget) -> (u32, Vec<Value>) {
    (
        POSITION_SLOT.get(widget).unwrap_or(0),
        ROW_KEY_SLOT.cloned(widget).map_or_else(Vec::new, |key| key.to_vec()),
    )
}
pub(super) fn pending_cell_value(widget: &gtk::Widget) -> Option<Value> {
    if IS_NULL_SLOT.get(widget).unwrap_or(false) {
        return Some(Value::Null);
    }
    FULL_EDIT_TEXT_SLOT.cloned(widget).map(Value::Text)
}
pub(super) fn copy_value_text(
    widget: &gtk::Widget,
    row: Option<&crate::ui::row_object::RowObject>,
    col: usize,
) -> String {
    if !IS_NULL_SLOT.get(widget).unwrap_or(false) {
        if let Some(text) = FULL_EDIT_TEXT_SLOT.cloned(widget) {
            return text;
        }
        if let Some(text) = row
            .and_then(|row| row.cell_value_in_row(col))
            .as_ref()
            .map(crate::ui::grid::display::value_to_copy_text)
        {
            return text;
        }
    }
    cell_text(widget)
}

pub(super) fn cell_text(widget: &gtk::Widget) -> String {
    if let Some(label) = widget.downcast_ref::<crate::ui::cell_editor::CellEditor>() {
        label.text().to_string()
    } else if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        label.text().to_string()
    } else {
        String::new()
    }
}
pub(super) fn row_at(view: &gtk::ColumnView, position: u32) -> Option<Vec<Value>> {
    row_object_at(view, position).map(|row| row.cells_clone())
}
pub(super) fn row_object_at(view: &gtk::ColumnView, position: u32) -> Option<crate::ui::row_object::RowObject> {
    view.model()?
        .item(position)?
        .downcast::<crate::ui::row_object::RowObject>()
        .ok()
}
pub(super) fn complete_row_at(view: &gtk::ColumnView, position: u32) -> Option<Vec<Value>> {
    let row = view
        .model()?
        .item(position)?
        .downcast::<crate::ui::row_object::RowObject>()
        .ok()?;
    row.complete_cells()
}
pub(super) fn rows_for_menu(view: &gtk::ColumnView, clicked: u32) -> Option<Vec<Vec<Value>>> {
    complete_rows(
        selected_row_positions(view, clicked)
            .into_iter()
            .map(|position| complete_row_at(view, position)),
    )
}
pub(super) fn values_for_column_menu(view: &gtk::ColumnView, clicked: u32, column: usize) -> Option<Vec<Vec<Value>>> {
    selected_row_positions(view, clicked)
        .into_iter()
        .map(|position| {
            let row = row_object_at(view, position)?;
            if !row.cell_is_loaded(column) {
                return None;
            }
            row.with_cell_value(column, |value| vec![value.clone()])
        })
        .collect()
}
pub(in crate::ui::grid) fn selected_row_positions(view: &gtk::ColumnView, clicked: u32) -> Vec<u32> {
    view.model()
        .and_then(|model| model.downcast::<gtk::MultiSelection>().ok())
        .map(|selection| selected_positions(&selection))
        .filter(|positions| !positions.is_empty())
        .unwrap_or_else(|| vec![clicked])
}
pub(super) fn complete_rows(rows: impl Iterator<Item = Option<Vec<Value>>>) -> Option<Vec<Vec<Value>>> {
    rows.collect()
}
pub(super) fn selected_positions(selection: &gtk::MultiSelection) -> Vec<u32> {
    let bitset = selection.selection();
    (0..bitset.size()).map(|index| bitset.nth(index as u32)).collect()
}
pub(super) fn select_row_for_menu(view: &gtk::ColumnView, position: u32) {
    if let Some(selection) = view
        .model()
        .and_then(|model| model.downcast::<gtk::MultiSelection>().ok())
        && position < selection.n_items()
        && !selection.is_selected(position)
    {
        selection.select_item(position, true);
    }
}
pub(super) fn export_snapshot(view: &gtk::ColumnView, columns: &[ColumnInfo], truncated: bool) -> Option<QueryResult> {
    Some(QueryResult {
        columns: columns.to_vec(),
        rows: complete_rows((0..view.model()?.n_items()).map(|position| complete_row_at(view, position)))?.into(),
        truncated,
    })
}
