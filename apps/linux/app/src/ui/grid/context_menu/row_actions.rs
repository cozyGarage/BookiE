use super::*;

pub(super) fn add_row_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    add_row_statement_actions(group, setup);
    add_export_action(group, setup);
}

pub(super) fn add_row_statement_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    let view = setup.column_view.downgrade();
    let action = |name: &str, statement: RowStatement| {
        let context = context.clone();
        let sender = sender.clone();
        let view = view.clone();
        gio::ActionEntry::builder(name)
            .activate(move |_, _, _| {
                if let Some(slot) = context.snapshot().as_ref() {
                    let Some(view) = view.upgrade() else { return };
                    let positions = selected_row_positions(&view, position(slot));
                    if positions.iter().any(|row| complete_row_at(&view, *row).is_none()) {
                        sender.send(GridMsg::IncompleteRowData).ok();
                        return;
                    }
                    sender.send(GridMsg::CopyRowAsStatement(positions, statement)).ok();
                }
            })
            .build()
    };
    group.add_action_entries([
        action("copy-row-insert", RowStatement::Insert),
        action("copy-row-update", RowStatement::Update),
        action("copy-row-delete", RowStatement::Delete),
        action("copy-row-where", RowStatement::Where),
    ]);
}

pub(super) fn add_mutation_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    add_insert_row_action(group, setup);
    add_set_cell_actions(group, setup);
    add_delete_row_action(group, setup);
    add_duplicate_row_action(group, setup);
}

pub(super) fn add_insert_row_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let sender = setup.sender.clone();
    group.add_action_entries([gio::ActionEntry::builder("insert-row")
        .activate(move |_, _, _| {
            sender.send(GridMsg::InsertRow).ok();
        })
        .build()]);
}

pub(super) fn add_set_cell_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    let set_null_action = gio::ActionEntry::builder("set-null")
        .activate(move |_, _, _| {
            if let Some(slot) = context.snapshot().as_ref() {
                let Some(row_position) = slot.row_position else { return };
                sender
                    .send(GridMsg::SetCellNull {
                        row_position,
                        col_index: slot.col_index,
                        row_key: slot.row_key.clone(),
                        draft_id: slot.draft_target.map(|(_, id)| id),
                    })
                    .ok();
            }
        })
        .build();
    group.add_action_entries([
        set_null_action,
        reset_cell_entry("set-default", Some(ColumnReset::Default), &setup.context, &setup.sender),
        reset_cell_entry("set-expression", None, &setup.context, &setup.sender),
    ]);
}

pub(super) fn add_delete_row_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    let view = setup.column_view.downgrade();
    group.add_action_entries([gio::ActionEntry::builder("delete-row")
        .activate(move |_, _, _| {
            let Some(slot) = context.snapshot() else { return };
            let Some(current_position) = slot.row_position else {
                return;
            };
            let current_draft = view
                .upgrade()
                .and_then(|view| row_object_at(&view, current_position))
                .and_then(|row| row.draft_id());
            let (row_position, draft_id) = match slot.draft_target {
                Some((position, id)) => (position, Some(id)),
                None if current_draft.is_some() => return,
                None => (current_position, None),
            };
            sender
                .send(GridMsg::DeleteRowAt {
                    row_position,
                    row_key: slot.row_key.clone(),
                    draft_id,
                })
                .ok();
        })
        .build()]);
}

pub(super) fn add_duplicate_row_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    let view = setup.column_view.downgrade();
    group.add_action_entries([gio::ActionEntry::builder("duplicate-row")
        .activate(move |_, _, _| {
            let Some(slot) = context.snapshot() else { return };
            let Some(view) = view.upgrade() else { return };
            let Some(row_position) = slot.row_position else { return };
            if complete_row_at(&view, row_position).is_none() {
                sender.send(GridMsg::IncompleteRowData).ok();
                return;
            }
            sender
                .send(GridMsg::DuplicateRow {
                    row_position,
                    row_key: slot.row_key.clone(),
                    draft_id: slot.draft_target.map(|(_, id)| id),
                })
                .ok();
        })
        .build()]);
}

pub(super) fn add_filter_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    group.add_action_entries([
        filter_by_value_action(
            setup.context.clone(),
            setup.sender.clone(),
            &setup.column_view,
            setup.columns.clone(),
            setup.local_result_view.clone(),
            false,
        ),
        filter_by_value_action(
            setup.context.clone(),
            setup.sender.clone(),
            &setup.column_view,
            setup.columns.clone(),
            setup.local_result_view.clone(),
            true,
        ),
    ]);
}

pub(super) fn add_whole_result_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    if !setup.whole_result_filter {
        return;
    }
    let Some(view) = setup.local_result_view.as_ref() else {
        return;
    };
    group.add_action_entries(whole_result::actions(
        setup.context.clone(),
        setup.sender.clone(),
        &setup.column_view,
        view.statement_identity(),
    ));
}

pub(super) fn add_column_selection_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    group.add_action_entries([crate::ui::grid::column_select::set_cells_entry(
        setup.context.clone(),
        setup.sender.clone(),
        &setup.column_view,
        setup.column_selection.clone(),
    )]);
}

pub(super) fn configure_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    set_action_enabled(group, "set-cells", setup.set_cells_enabled);
    set_action_enabled(group, "export", setup.snapshot_export);
    let supports_sql = bookie_core::export::supports_sql_literals(&setup.driver_id);
    set_action_enabled(group, "copy-in-clause", supports_sql);
    set_action_enabled(group, "copy-values-list", supports_sql);
}

pub(super) fn set_action_enabled(group: &gio::SimpleActionGroup, name: &str, enabled: bool) {
    if let Some(action) = group
        .lookup_action(name)
        .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
    {
        action.set_enabled(enabled);
    }
}
