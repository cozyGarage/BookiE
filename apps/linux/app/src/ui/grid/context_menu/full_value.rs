//! Seeding and targeting of the full-value editor from a cell.

use super::*;

pub(super) enum TextSeed {
    Value(String, crate::ui::grid::full_value_editor::FullValueKind),
    TooLarge,
    Unsupported,
}

pub(super) fn full_value_seed(
    widget: &gtk::Widget,
    row: &crate::ui::row_object::RowObject,
    column: usize,
    data_type: &str,
) -> TextSeed {
    if IS_NULL_SLOT.get(widget).unwrap_or(false) {
        return crate::ui::grid::full_value_editor::edit_kind(data_type, &Value::Null)
            .map(|kind| TextSeed::Value(String::new(), kind))
            .unwrap_or(TextSeed::Unsupported);
    }
    if let Some(size) = FULL_EDIT_TEXT_SLOT.with(widget, String::len) {
        if !crate::ui::grid::full_value_editor::within_edit_budget(size) {
            return TextSeed::TooLarge;
        }
        let Some(text) = FULL_EDIT_TEXT_SLOT.cloned(widget) else {
            return TextSeed::Unsupported;
        };
        let Some(kind) = FULL_EDIT_KIND_SLOT.get(widget) else {
            return TextSeed::Unsupported;
        };
        return TextSeed::Value(text, kind);
    }
    row.with_cell_value(column, |value| match value {
        Value::Text(text) if crate::ui::grid::full_value_editor::within_edit_budget(text.len()) => {
            crate::ui::grid::full_value_editor::edit_kind(data_type, value)
                .map(|kind| TextSeed::Value(text.clone(), kind))
                .unwrap_or(TextSeed::Unsupported)
        }
        Value::Text(_) => TextSeed::TooLarge,
        value => crate::ui::grid::full_value_editor::edit_kind(data_type, value)
            .map(|kind| {
                let text = crate::ui::grid::display::value_to_full_edit_text(value);
                if crate::ui::grid::full_value_editor::within_edit_budget(text.len()) {
                    TextSeed::Value(text, kind)
                } else {
                    TextSeed::TooLarge
                }
            })
            .unwrap_or(TextSeed::Unsupported),
    })
    .unwrap_or(TextSeed::Unsupported)
}

pub(super) fn row_has_full_text_value(
    view: &gtk::ColumnView,
    widget: &gtk::Widget,
    position: u32,
    column: usize,
) -> bool {
    let Some(row) = row_object_at(view, position) else {
        return false;
    };
    let (_, row_key) = cell_row_identity(widget);
    (row.draft_id().is_some() || has_persisted_row_key(&row_key))
        && (row.cell_is_partial(column)
            || row
                .with_cell_value(column, |value| {
                    matches!(
                        value,
                        Value::Text(_) | Value::Json(_) | Value::JsonText(_) | Value::Null
                    )
                })
                .unwrap_or(false))
}

pub(super) fn has_persisted_row_key(row_key: &[Value]) -> bool {
    crate::services::change_tracker::RowKey::from_pk_values(row_key).is_some()
}

pub(super) fn full_value_target(
    view: &gtk::ColumnView,
    slot: &CellContext,
    columns: &[ColumnInfo],
) -> Option<(u32, crate::ui::row_object::RowObject)> {
    let position = slot.row_position?;
    if POSITION_SLOT.get(&slot.widget) != Some(position) || cell_row_identity(&slot.widget).1 != slot.row_key {
        return None;
    }
    let row = row_object_at(view, position)?;
    if row.draft_id() != slot.draft_target.map(|(_, id)| id) {
        return None;
    }
    if row.draft_id().is_none() {
        let keys: Vec<_> = columns
            .iter()
            .enumerate()
            .filter_map(|(index, column)| column.primary_key.then_some(index))
            .collect();
        if !has_persisted_row_key(&slot.row_key) || row.row_identity(&keys).pk_values.as_ref() != slot.row_key {
            return None;
        }
    }
    Some((position, row))
}
