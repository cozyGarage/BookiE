use super::*;

pub(super) fn add_value_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    add_copy_value_action(group, setup);
    add_copy_column_name_action(group, setup);
    add_view_value_action(group, setup);
    add_show_row_json_action(group, setup);
}

pub(super) fn add_copy_value_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    let view = setup.column_view.downgrade();
    group.add_action_entries([gio::ActionEntry::builder("copy-value")
        .activate(move |_, _, _| {
            if let Some(slot) = context.snapshot().as_ref() {
                let Some(view) = view.upgrade() else { return };
                if !menu_target_matches(&view, slot) {
                    return;
                }
                let row = row_object_at(&view, position(slot));
                if row.as_ref().is_some_and(|row| row.cell_is_partial(slot.col_index)) {
                    let (_, row_key) = cell_row_identity(&slot.widget);
                    if !row_key.is_empty() {
                        sender
                            .send(GridMsg::FetchCellValue {
                                col_index: slot.col_index,
                                column_name: slot.column_name.clone(),
                                row_key,
                                edit_row_position: None,
                            })
                            .ok();
                    }
                    return;
                }
                let text = copy_value_text(&slot.widget, row.as_ref(), slot.col_index);
                sender.send(GridMsg::CopyToClipboard(text)).ok();
            }
        })
        .build()]);
}

pub(super) fn add_copy_column_name_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    group.add_action_entries([gio::ActionEntry::builder("copy-column-name")
        .activate(move |_, _, _| {
            if let Some(slot) = context.snapshot().as_ref() {
                sender.send(GridMsg::CopyToClipboard(slot.column_name.clone())).ok();
            }
        })
        .build()]);
}

pub(super) fn add_view_value_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    let view = setup.column_view.downgrade();
    let columns = setup.columns.clone();
    group.add_action_entries([gio::ActionEntry::builder("view-value")
        .activate(move |_, _, _| {
            let context = context.snapshot();
            let Some(slot) = context.as_ref() else { return };
            let Some(view) = view.upgrade() else { return };
            if !menu_target_matches(&view, slot) {
                return;
            }
            let Some(column) = columns.get(slot.col_index) else {
                return;
            };
            let row = row_object_at(&view, position(slot));
            if let Some(value) = pending_cell_value(&slot.widget) {
                let null_copy_text =
                    matches!(&value, Value::Null).then(|| copy_value_text(&slot.widget, row.as_ref(), slot.col_index));
                value_viewer::present(&view, column, value, null_copy_text);
                return;
            }
            show_loaded_cell_value(&view, &sender, slot, row, column);
        })
        .build()]);
}

pub(super) fn show_loaded_cell_value(
    view: &gtk::ColumnView,
    sender: &relm4::Sender<GridMsg>,
    slot: &CellContext,
    row: Option<crate::ui::row_object::RowObject>,
    column: &ColumnInfo,
) {
    if row
        .as_ref()
        .is_some_and(|row| row.cell_preview(slot.col_index).is_some())
    {
        let (_, row_key) = cell_row_identity(&slot.widget);
        if row_key.is_empty() {
            let Some(value) = source_value_at(view, position(slot), slot.col_index) else {
                return;
            };
            value_viewer::present(view, column, value, None);
        } else {
            sender
                .send(GridMsg::FetchCellValue {
                    col_index: slot.col_index,
                    column_name: slot.column_name.clone(),
                    row_key,
                    edit_row_position: None,
                })
                .ok();
        }
        return;
    }
    let Some(value) = row.and_then(|row| row.cell_value_in_row(slot.col_index)) else {
        return;
    };
    value_viewer::present(view, column, value, None);
}

pub(super) fn add_show_row_json_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    let view = setup.column_view.downgrade();
    let columns = setup.columns.clone();
    group.add_action_entries([gio::ActionEntry::builder("show-row-json")
        .activate(move |_, _, _| {
            let context = context.snapshot();
            let Some(slot) = context.as_ref() else { return };
            let Some(view) = view.upgrade() else { return };
            if !menu_target_matches(&view, slot) {
                return;
            }
            let Some(row) = complete_row_at(&view, position(slot)) else {
                sender.send(GridMsg::IncompleteRowData).ok();
                return;
            };
            let json = export::render_row_json(&columns, &row);
            sender.send(GridMsg::ShowRowAsJson(json)).ok();
        })
        .build()]);
}

pub(super) fn add_export_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let sender = setup.sender.clone();
    let view = setup.column_view.downgrade();
    let columns = setup.columns.clone();
    let truncated = setup.truncated;
    group.add_action_entries([gio::ActionEntry::builder("export")
        .activate(move |_, _, _| {
            let Some(view) = view.upgrade() else { return };
            if let Some(result) = export_snapshot(&view, &columns, truncated) {
                sender.send(GridMsg::ExportResults(std::sync::Arc::new(result))).ok();
            } else {
                sender.send(GridMsg::IncompleteRowData).ok();
            }
        })
        .build()]);
}

pub(super) fn add_loaded_export_action(
    group: &gio::SimpleActionGroup,
    setup: &MenuActionSetup,
) -> Option<(gio::SimpleAction, LocalResultView)> {
    let local_result_view = setup.local_result_view.clone()?;
    let action = gio::SimpleAction::new("export-loaded", None);
    let sender = setup.sender.clone();
    let view = local_result_view.clone();
    action.connect_activate(move |_, _| {
        sender.send(GridMsg::ExportResults(view.loaded_result())).ok();
    });
    group.add_action(&action);
    Some((action, local_result_view))
}
