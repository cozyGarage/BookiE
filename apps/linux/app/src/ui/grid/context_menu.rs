mod keyboard_menu;
mod pointer_menu;
mod whole_result;

use bookie_core::sql_dialect::ColumnReset;
use bookie_core::sql_literal::RowStatement;
use std::rc::Rc;

use bookie_core::{ColumnInfo, QueryResult, Value};
use gtk4::prelude::*;
use gtk4::{self as gtk, gio};

use super::display::{FULL_EDIT_KIND_SLOT, FULL_EDIT_TEXT_SLOT, IS_NULL_SLOT, POSITION_SLOT, ROW_KEY_SLOT};
use super::editing::enter_edit_mode;
use super::export;
use super::local_filter::LocalResultView;
use super::value_viewer;
use super::{GridMsg, ResetCellRequest};

const CELL_MENU_TRIGGER: &str = "Menu|<Shift>F10";

use super::cell_context::{CellContext, CellContextStore};

struct MenuActionSetup {
    context: CellContextStore,
    sender: relm4::Sender<GridMsg>,
    column_view: gtk::ColumnView,
    columns: Rc<Vec<ColumnInfo>>,
    truncated: bool,
    driver_id: String,
    local_result_view: Option<LocalResultView>,
    column_selection: super::column_select::ColumnSelectionHandle,
    whole_result_filter: bool,
    set_cells_enabled: bool,
    snapshot_export: bool,
}

struct MenuActionBindings {
    edit_action: gio::SimpleAction,
    edit_full_value_action: gio::SimpleAction,
    reference_action: gio::SimpleAction,
    show_referencing_action: gio::SimpleAction,
    export_loaded: Option<(gio::SimpleAction, LocalResultView)>,
}

struct MenuModels {
    editable: gio::Menu,
    readonly: gio::Menu,
    empty: gio::Menu,
}

struct MenuPopovers {
    editable: gtk::PopoverMenu,
    readonly: gtk::PopoverMenu,
    empty: gtk::PopoverMenu,
}

#[derive(Clone)]
pub(super) struct GridMenus {
    context: CellContextStore,
    columns: Rc<Vec<ColumnInfo>>,
    cell_menu_trigger: gtk::ShortcutTrigger,
    editable_popover: gtk::PopoverMenu,
    readonly_popover: gtk::PopoverMenu,
    edit_action: gio::SimpleAction,
    edit_full_value_action: gio::SimpleAction,
    reference_action: gio::SimpleAction,
    show_referencing_action: gio::SimpleAction,
    export_loaded: Option<(gio::SimpleAction, LocalResultView)>,
    foreign_key_columns: Rc<std::collections::HashSet<String>>,
    has_inbound_foreign_keys: bool,
}

pub(super) struct GridMenuOptions {
    pub(super) filterable: bool,
    pub(super) row_mutations_allowed: bool,
    pub(super) foreign_key_columns: std::collections::HashSet<String>,
    pub(super) has_inbound_foreign_keys: bool,
    pub(super) local_result_view: Option<LocalResultView>,
    pub(super) whole_result_filter: bool,
    pub(super) snapshot_export: bool,
    pub(super) column_selection: super::column_select::ColumnSelectionHandle,
    pub(super) set_cells_enabled: bool,
}

fn reset_cell_entry(
    name: &str,
    reset: Option<ColumnReset>,
    context: &CellContextStore,
    sender: &relm4::Sender<GridMsg>,
) -> gio::ActionEntry<gio::SimpleActionGroup> {
    let context = context.clone();
    let sender = sender.clone();
    gio::ActionEntry::builder(name)
        .activate(move |_, _, _| {
            let snapshot = context.snapshot();
            let Some(slot) = snapshot.as_ref() else {
                return;
            };
            let Some(row_position) = slot.row_position else { return };
            if slot.draft_target.is_some() {
                return;
            }
            let request = ResetCellRequest {
                row_position,
                col_index: slot.col_index,
                row_key: slot.row_key.clone(),
                reset: reset.clone(),
            };
            sender.send(GridMsg::ResetCell(request)).ok();
        })
        .build()
}

pub(super) fn install_grid_context_menus(
    column_view: &gtk::ColumnView,
    sender: relm4::Sender<GridMsg>,
    result_columns: &[ColumnInfo],
    truncated: bool,
    driver_id: String,
    options: GridMenuOptions,
) -> GridMenus {
    let context = CellContextStore::default();
    let columns = Rc::new(result_columns.to_vec());
    let models = build_menu_models(&options);
    let group = gio::SimpleActionGroup::new();
    let setup = MenuActionSetup {
        context: context.clone(),
        sender,
        column_view: column_view.clone(),
        columns: columns.clone(),
        truncated,
        driver_id,
        local_result_view: options.local_result_view.clone(),
        column_selection: options.column_selection.clone(),
        whole_result_filter: options.whole_result_filter,
        set_cells_enabled: options.set_cells_enabled,
        snapshot_export: options.snapshot_export,
    };
    let actions = install_actions(&group, &setup);
    column_view.insert_action_group("cell", Some(&group));
    configure_actions(&group, &setup);
    let popovers = build_menu_popovers(column_view, models);
    let menus = GridMenus {
        context,
        columns,
        cell_menu_trigger: crate::ui::shortcut::parse(CELL_MENU_TRIGGER),
        editable_popover: popovers.editable,
        readonly_popover: popovers.readonly,
        edit_action: actions.edit_action,
        edit_full_value_action: actions.edit_full_value_action,
        reference_action: actions.reference_action,
        show_referencing_action: actions.show_referencing_action,
        export_loaded: actions.export_loaded,
        foreign_key_columns: Rc::new(options.foreign_key_columns),
        has_inbound_foreign_keys: options.has_inbound_foreign_keys,
    };
    keyboard_menu::install(column_view, &menus);
    pointer_menu::install(column_view, &menus, popovers.empty);
    menus
}

fn build_menu_models(options: &GridMenuOptions) -> MenuModels {
    let local_result = options.local_result_view.is_some();
    let editable = build_menu(
        true,
        options.row_mutations_allowed,
        true,
        options.filterable,
        local_result,
    );
    let readonly = build_menu(false, false, true, options.filterable, local_result);
    if options.whole_result_filter {
        whole_result::extend_menu(&editable);
        whole_result::extend_menu(&readonly);
    }
    MenuModels {
        editable,
        readonly,
        empty: empty_grid_menu(options.row_mutations_allowed),
    }
}

fn install_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) -> MenuActionBindings {
    let (edit_action, edit_full_value_action) = install_edit_actions(group, setup);
    let export_loaded = add_loaded_export_action(group, setup);
    add_copy_actions(group, setup);
    add_value_actions(group, setup);
    add_row_actions(group, setup);
    add_mutation_actions(group, setup);
    add_filter_actions(group, setup);
    add_whole_result_actions(group, setup);
    add_column_selection_action(group, setup);
    let reference_action = go_to_reference_action(
        setup.context.clone(),
        setup.sender.clone(),
        &setup.column_view,
        setup.columns.clone(),
    );
    let show_referencing_action = show_referencing_action(
        setup.context.clone(),
        setup.sender.clone(),
        &setup.column_view,
        setup.columns.clone(),
    );
    group.add_action(&reference_action);
    group.add_action(&show_referencing_action);
    MenuActionBindings {
        edit_action,
        edit_full_value_action,
        reference_action,
        show_referencing_action,
        export_loaded,
    }
}

fn install_edit_actions(
    group: &gio::SimpleActionGroup,
    setup: &MenuActionSetup,
) -> (gio::SimpleAction, gio::SimpleAction) {
    let edit_action = build_edit_action(setup);
    let edit_full_value_action = build_edit_full_value_action(setup);
    group.add_action(&edit_action);
    group.add_action(&edit_full_value_action);
    (edit_action, edit_full_value_action)
}

fn build_edit_action(setup: &MenuActionSetup) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("edit", None);
    let context = setup.context.clone();
    let view = setup.column_view.downgrade();
    let columns = setup.columns.clone();
    action.connect_activate(move |_, _| {
        if let Some(slot) = context.snapshot().as_ref()
            && let Some(view) = view.upgrade()
            && full_value_target(&view, slot, &columns).is_some()
            && let Some(label) = slot.widget.downcast_ref::<crate::ui::cell_editor::CellEditor>()
        {
            enter_edit_mode(label);
        }
    });
    action
}

fn build_edit_full_value_action(setup: &MenuActionSetup) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("edit-full-value", None);
    let context = setup.context.clone();
    let view = setup.column_view.downgrade();
    let columns = setup.columns.clone();
    let sender = setup.sender.clone();
    action.connect_activate(move |_, _| {
        let context = context.snapshot();
        let Some(slot) = context.as_ref() else { return };
        let Some(view) = view.upgrade() else { return };
        let Some(column) = columns.get(slot.col_index) else {
            return;
        };
        if !super::full_value_editor::is_full_value_column(&column.data_type) {
            return;
        }
        let Some((row_position, row)) = full_value_target(&view, slot, &columns) else {
            return;
        };
        let row_key = slot.row_key.clone();
        if row.cell_is_partial(slot.col_index) {
            sender
                .send(GridMsg::FetchCellValue {
                    col_index: slot.col_index,
                    column_name: slot.column_name.clone(),
                    row_key,
                    edit_row_position: Some(row_position),
                })
                .ok();
            return;
        }
        let (initial_text, kind) = match full_value_seed(&slot.widget, &row, slot.col_index, &column.data_type) {
            TextSeed::Value(text, kind) => (text, kind),
            TextSeed::TooLarge => {
                super::full_value_editor::present_too_large(&view);
                return;
            }
            TextSeed::Unsupported => return,
        };
        super::full_value_editor::present(
            &view,
            &slot.column_name,
            &initial_text,
            super::full_value_editor::EditContext {
                row_position,
                col_index: slot.col_index,
                row_key,
                draft_id: row.draft_id(),
                nullable: column.nullable,
                kind,
                sender: sender.clone(),
            },
        );
    });
    action
}

fn add_copy_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    add_text_copy_actions(group, setup);
    add_structured_copy_actions(group, setup);
    add_copy_in_clause_action(group, setup);
}

fn add_text_copy_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = &setup.context;
    let sender = &setup.sender;
    let column_view = &setup.column_view;
    let columns = &setup.columns;
    let selection = &setup.column_selection;
    group.add_action_entries([
        copy_rows_action(
            "copy-rows",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| export::render_tsv(columns, rows, false),
        ),
        copy_rows_action(
            "copy-rows-headers",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| export::render_tsv(columns, rows, true),
        ),
        copy_rows_action(
            "copy-csv",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| export::render_csv(columns, rows, false),
        ),
        copy_rows_action(
            "copy-csv-headers",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| export::render_csv(columns, rows, true),
        ),
        copy_rows_action(
            "copy-text-table",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            export::render_text_table,
        ),
    ]);
}

fn add_structured_copy_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = &setup.context;
    let sender = &setup.sender;
    let column_view = &setup.column_view;
    let columns = &setup.columns;
    let selection = &setup.column_selection;
    let driver_id = setup.driver_id.clone();
    group.add_action_entries([
        copy_rows_action(
            "copy-json",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            export::render_json,
        ),
        copy_rows_action(
            "copy-markdown",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            export::render_markdown,
        ),
        copy_rows_action(
            "copy-json-lines",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            export::render_json_lines,
        ),
        copy_rows_action(
            "copy-values-list",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            move |columns, rows| export::render_value_list(&driver_id, columns, rows),
        ),
    ]);
}

fn add_copy_in_clause_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let selection = setup.column_selection.clone();
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    let view = setup.column_view.downgrade();
    let driver_id = setup.driver_id.clone();
    group.add_action_entries([gio::ActionEntry::builder("copy-in-clause")
        .activate(move |_, _, _| {
            let context = context.snapshot();
            let Some(slot) = context.as_ref() else { return };
            let Some(view) = view.upgrade() else { return };
            let column_rows = if selection.borrow().contains(slot.col_index) {
                super::column_select::project_rows(
                    &[slot.col_index],
                    0..view.model().map_or(0, |model| model.n_items()),
                    |position| row_object_at(&view, position),
                )
            } else {
                values_for_column_menu(&view, position(slot), slot.col_index)
            };
            let Some(rows) = column_rows else {
                sender.send(GridMsg::IncompleteRowData).ok();
                return;
            };
            let clause = export::render_in_clause(&driver_id, &rows, 0);
            show_in_clause_skips(&view, clause.skipped);
            if !clause.sql.is_empty() {
                sender.send(GridMsg::CopyToClipboard(clause.sql)).ok();
            }
        })
        .build()]);
}

fn show_in_clause_skips(view: &gtk::ColumnView, skipped: usize) {
    if skipped == 0 {
        return;
    }
    let message = bookie_app_services::tr!("Skipped {n} NULL, binary or unsupported values.")
        .replace("{n}", &skipped.to_string());
    let alert = libadwaita::AlertDialog::new(Some(&bookie_app_services::tr!("Copy as IN clause")), Some(&message));
    use libadwaita::prelude::*;
    alert.add_response("close", &bookie_app_services::tr!("Close"));
    alert.set_close_response("close");
    alert.present(Some(view));
}

fn add_value_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    add_copy_value_action(group, setup);
    add_copy_column_name_action(group, setup);
    add_view_value_action(group, setup);
    add_show_row_json_action(group, setup);
}

fn add_copy_value_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
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

fn add_copy_column_name_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
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

fn add_view_value_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    let view = setup.column_view.downgrade();
    group.add_action_entries([gio::ActionEntry::builder("view-value")
        .activate(move |_, _, _| {
            let context = context.snapshot();
            let Some(slot) = context.as_ref() else { return };
            let Some(view) = view.upgrade() else { return };
            if !menu_target_matches(&view, slot) {
                return;
            }
            let row = row_object_at(&view, position(slot));
            if let Some(value) = pending_cell_value(&slot.widget) {
                let null_copy_text =
                    matches!(&value, Value::Null).then(|| copy_value_text(&slot.widget, row.as_ref(), slot.col_index));
                value_viewer::present(&view, &slot.column_name, value, null_copy_text);
                return;
            }
            show_loaded_cell_value(&view, &sender, slot, row);
        })
        .build()]);
}

fn show_loaded_cell_value(
    view: &gtk::ColumnView,
    sender: &relm4::Sender<GridMsg>,
    slot: &CellContext,
    row: Option<crate::ui::row_object::RowObject>,
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
            value_viewer::present(view, &slot.column_name, value, None);
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
    value_viewer::present(view, &slot.column_name, value, None);
}

fn add_show_row_json_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
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

fn add_export_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
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

fn add_loaded_export_action(
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

fn add_row_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    add_row_statement_actions(group, setup);
    add_export_action(group, setup);
}

fn add_row_statement_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
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

fn add_mutation_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    add_insert_row_action(group, setup);
    add_set_cell_actions(group, setup);
    add_delete_row_action(group, setup);
    add_duplicate_row_action(group, setup);
}

fn add_insert_row_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let sender = setup.sender.clone();
    group.add_action_entries([gio::ActionEntry::builder("insert-row")
        .activate(move |_, _, _| {
            sender.send(GridMsg::InsertRow).ok();
        })
        .build()]);
}

fn add_set_cell_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
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

fn add_delete_row_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
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

fn add_duplicate_row_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
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

fn add_filter_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
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

fn add_whole_result_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
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

fn add_column_selection_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    group.add_action_entries([super::column_select::set_cells_entry(
        setup.context.clone(),
        setup.sender.clone(),
        &setup.column_view,
        setup.column_selection.clone(),
    )]);
}

fn configure_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    set_action_enabled(group, "set-cells", setup.set_cells_enabled);
    set_action_enabled(group, "export", setup.snapshot_export);
    let supports_sql = bookie_core::export::supports_sql_literals(&setup.driver_id);
    set_action_enabled(group, "copy-in-clause", supports_sql);
    set_action_enabled(group, "copy-values-list", supports_sql);
}

fn set_action_enabled(group: &gio::SimpleActionGroup, name: &str, enabled: bool) {
    if let Some(action) = group
        .lookup_action(name)
        .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
    {
        action.set_enabled(enabled);
    }
}

fn build_menu_popovers(column_view: &gtk::ColumnView, models: MenuModels) -> MenuPopovers {
    let editable = gtk::PopoverMenu::from_model_full(&models.editable, gtk::PopoverMenuFlags::NESTED);
    let readonly = gtk::PopoverMenu::from_model_full(&models.readonly, gtk::PopoverMenuFlags::NESTED);
    let empty = crate::ui::context_menu::from_model(Some(&models.empty));
    crate::ui::context_menu::configure(&editable);
    crate::ui::context_menu::configure(&readonly);
    editable.set_parent(column_view);
    readonly.set_parent(column_view);
    empty.set_parent(column_view);
    let editable_for_destroy = editable.clone();
    let readonly_for_destroy = readonly.clone();
    let empty_for_destroy = empty.clone();
    column_view.connect_destroy(move |_| {
        editable_for_destroy.unparent();
        readonly_for_destroy.unparent();
        empty_for_destroy.unparent();
    });
    MenuPopovers {
        editable,
        readonly,
        empty,
    }
}

fn empty_grid_menu(row_mutations_allowed: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    if row_mutations_allowed {
        menu.append(Some(&bookie_app_services::tr!("Insert row")), Some("cell.insert-row"));
    }
    menu.append(
        Some(&bookie_app_services::tr!("Jump to Column…")),
        Some("grid.jump-column"),
    );
    menu
}

fn set_cells_item() -> gio::MenuItem {
    let item = gio::MenuItem::new(
        Some(&bookie_app_services::tr!("Set Cells to Value…")),
        Some("cell.set-cells"),
    );
    item.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
    item
}

fn export_loaded_item() -> gio::MenuItem {
    let item = gio::MenuItem::new(
        Some(&bookie_app_services::tr!("Export All Loaded Rows…")),
        Some("cell.export-loaded"),
    );
    item.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
    item
}

fn build_copy_as_menu(insert_copy: bool) -> gio::Menu {
    let copy_as = gio::Menu::new();
    copy_as.append(Some(&bookie_app_services::tr!("Rows")), Some("cell.copy-rows"));
    copy_as.append(
        Some(&bookie_app_services::tr!("With Headers")),
        Some("cell.copy-rows-headers"),
    );
    copy_as.append(
        Some(&bookie_app_services::tr!("Text Table")),
        Some("cell.copy-text-table"),
    );
    copy_as.append(Some(&bookie_app_services::tr!("JSON")), Some("cell.copy-json"));
    copy_as.append(
        Some(&bookie_app_services::tr!("JSON Lines")),
        Some("cell.copy-json-lines"),
    );
    copy_as.append(Some(&bookie_app_services::tr!("CSV")), Some("cell.copy-csv"));
    copy_as.append(
        Some(&bookie_app_services::tr!("CSV with Headers")),
        Some("cell.copy-csv-headers"),
    );
    copy_as.append(Some(&bookie_app_services::tr!("Markdown")), Some("cell.copy-markdown"));
    copy_as.append(
        Some(&bookie_app_services::tr!("IN Clause")),
        Some("cell.copy-in-clause"),
    );
    copy_as.append(
        Some(&bookie_app_services::tr!("Values List")),
        Some("cell.copy-values-list"),
    );
    if insert_copy {
        copy_as.append(
            Some(&bookie_app_services::tr!("INSERT Statement")),
            Some("cell.copy-row-insert"),
        );
        copy_as.append(
            Some(&bookie_app_services::tr!("UPDATE Statement")),
            Some("cell.copy-row-update"),
        );
        copy_as.append(
            Some(&bookie_app_services::tr!("DELETE Statement")),
            Some("cell.copy-row-delete"),
        );
        copy_as.append(
            Some(&bookie_app_services::tr!("WHERE Clause")),
            Some("cell.copy-row-where"),
        );
    }
    copy_as
}

fn edit_section() -> gio::Menu {
    let section = gio::Menu::new();
    let item = gio::MenuItem::new(Some(&bookie_app_services::tr!("Edit cell")), Some("cell.edit"));
    item.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
    section.append_item(&item);
    let full_value = gio::MenuItem::new(
        Some(&bookie_app_services::tr!("Edit Full Value…")),
        Some("cell.edit-full-value"),
    );
    full_value.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
    section.append_item(&full_value);
    section
}

fn display_section(filterable: bool, local_result: bool) -> gio::Menu {
    let display = gio::Menu::new();
    if filterable || local_result {
        display.append(
            Some(&bookie_app_services::tr!("Filter by This Value")),
            Some("cell.filter-by-value"),
        );
        display.append(
            Some(&bookie_app_services::tr!("Exclude This Value")),
            Some("cell.exclude-value"),
        );
    }
    if filterable {
        display.append(
            Some(&bookie_app_services::tr!("Go to Referenced Row")),
            Some("cell.go-to-reference"),
        );
        display.append(
            Some(&bookie_app_services::tr!("Show Referencing Rows")),
            Some("cell.show-referencing-rows"),
        );
    }
    display.append(Some(&bookie_app_services::tr!("View Value…")), Some("cell.view-value"));
    display.append(
        Some(&bookie_app_services::tr!("Show Row as JSON")),
        Some("cell.show-row-json"),
    );
    display
}

fn build_menu(
    editable: bool,
    row_operations: bool,
    insert_copy: bool,
    filterable: bool,
    local_result: bool,
) -> gio::Menu {
    let menu = gio::Menu::new();
    if editable {
        menu.append_section(None, &edit_section());
    }
    let copy = gio::Menu::new();
    copy.append(Some(&bookie_app_services::tr!("Copy value")), Some("cell.copy-value"));
    copy.append_submenu(
        Some(&bookie_app_services::tr!("Copy as")),
        &build_copy_as_menu(insert_copy),
    );
    copy.append(
        Some(&bookie_app_services::tr!("Copy column name")),
        Some("cell.copy-column-name"),
    );
    menu.append_section(None, &copy);
    menu.append_section(None, &display_section(filterable, local_result));
    let actions = gio::Menu::new();
    let export_label = if local_result {
        bookie_app_services::tr!("Export Visible Rows…")
    } else {
        bookie_app_services::tr!("Export Results…")
    };
    actions.append(Some(&export_label), Some("cell.export"));
    if local_result {
        actions.append_item(&export_loaded_item());
    }
    actions.append(Some(&bookie_app_services::tr!("Columns…")), Some("grid.columns"));
    actions.append(
        Some(&bookie_app_services::tr!("Jump to Column…")),
        Some("grid.jump-column"),
    );
    actions.append(
        Some(&bookie_app_services::tr!("Find in Loaded Rows…")),
        Some("grid.find-row"),
    );
    actions.append(
        Some(&bookie_app_services::tr!("Highlight Rules…")),
        Some("grid.highlight-rules"),
    );
    if row_operations {
        actions.append(Some(&bookie_app_services::tr!("Insert row")), Some("cell.insert-row"));
        actions.append(
            Some(&bookie_app_services::tr!("Duplicate row")),
            Some("cell.duplicate-row"),
        );
        actions.append(Some(&bookie_app_services::tr!("Set to NULL")), Some("cell.set-null"));
        actions.append_item(&set_cells_item());
        actions.append(
            Some(&bookie_app_services::tr!("Set to Default")),
            Some("cell.set-default"),
        );
        actions.append(
            Some(&bookie_app_services::tr!("Set to Expression…")),
            Some("cell.set-expression"),
        );
        actions.append(Some(&bookie_app_services::tr!("Delete row")), Some("cell.delete-row"));
    }
    menu.append_section(None, &actions);
    menu
}

fn go_to_reference_action(
    context: CellContextStore,
    sender: relm4::Sender<GridMsg>,
    column_view: &gtk::ColumnView,
    columns: Rc<Vec<ColumnInfo>>,
) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("go-to-reference", None);
    action.set_enabled(false);
    let view = column_view.downgrade();
    action.connect_activate(move |_, _| {
        let context = context.snapshot();
        let Some(slot) = context.as_ref() else { return };
        let Some(view) = view.upgrade() else { return };
        if !menu_target_matches(&view, slot) {
            return;
        }
        let Some(cells) = row_at(&view, position(slot)) else {
            return;
        };
        let row = columns
            .iter()
            .zip(cells)
            .map(|(column, value)| (column.name.clone(), value))
            .collect();
        sender
            .send(GridMsg::GoToReference {
                column: slot.column_name.clone(),
                row,
            })
            .ok();
    });
    action
}

fn show_referencing_action(
    context: CellContextStore,
    sender: relm4::Sender<GridMsg>,
    column_view: &gtk::ColumnView,
    columns: Rc<Vec<ColumnInfo>>,
) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("show-referencing-rows", None);
    action.set_enabled(false);
    let view = column_view.downgrade();
    action.connect_activate(move |_, _| {
        let context = context.snapshot();
        let Some(slot) = context.as_ref() else { return };
        let Some(view) = view.upgrade() else { return };
        if !menu_target_matches(&view, slot) {
            return;
        }
        let Some(cells) = row_at(&view, position(slot)) else {
            return;
        };
        let row = columns
            .iter()
            .zip(cells)
            .map(|(column, value)| (column.name.clone(), value))
            .collect();
        sender.send(GridMsg::ShowReferencingRows { row }).ok();
    });
    action
}

fn filter_by_value_action(
    context: CellContextStore,
    sender: relm4::Sender<GridMsg>,
    column_view: &gtk::ColumnView,
    columns: Rc<Vec<ColumnInfo>>,
    local_result_view: Option<LocalResultView>,
    exclude: bool,
) -> gio::ActionEntry<gio::SimpleActionGroup> {
    let view = column_view.downgrade();
    let name = if exclude { "exclude-value" } else { "filter-by-value" };
    gio::ActionEntry::builder(name)
        .activate(move |_, _, _| {
            let context = context.snapshot();
            let Some(slot) = context.as_ref() else { return };
            let Some(view) = view.upgrade() else { return };
            if !menu_target_matches(&view, slot) {
                return;
            }
            let value = match &local_result_view {
                Some(_) => source_value_at(&view, position(slot), slot.col_index),
                None => row_at(&view, position(slot)).and_then(|row| row.get(slot.col_index).cloned()),
            };
            let Some(value) = value else { return };
            if let Some(local_result_view) = &local_result_view {
                if !local_result_view.filter_by_value(slot.col_index, value, exclude) {
                    sender
                        .send(GridMsg::ShowToast(bookie_app_services::tr!(
                            "Values of this type can't be used as a filter"
                        )))
                        .ok();
                }
                return;
            }
            let Some(column) = columns.get(slot.col_index) else {
                return;
            };
            sender
                .send(GridMsg::FilterByValue {
                    column: column.name.clone(),
                    value,
                    exclude,
                })
                .ok();
        })
        .build()
}

fn source_value_at(view: &gtk::ColumnView, position: u32, column: usize) -> Option<Value> {
    let model = view.model()?.downcast::<gtk::MultiSelection>().ok()?.model()?;
    if let Some(store) = model.downcast_ref::<crate::ui::row_store::RowStore>() {
        return store.source_value_at(position, column);
    }
    model
        .item(position)?
        .downcast::<crate::ui::row_object::RowObject>()
        .ok()?
        .with_cell_value(column, Clone::clone)
}

fn copy_rows_action<F>(
    name: &'static str,
    context: CellContextStore,
    sender: relm4::Sender<GridMsg>,
    column_view: &gtk::ColumnView,
    columns: Rc<Vec<ColumnInfo>>,
    selection: super::column_select::ColumnSelectionHandle,
    render: F,
) -> gio::ActionEntry<gio::SimpleActionGroup>
where
    F: Fn(&[ColumnInfo], &[Vec<Value>]) -> String + 'static,
{
    let view = column_view.downgrade();
    gio::ActionEntry::builder(name)
        .activate(move |_, _, _| {
            let context = context.snapshot();
            let Some(slot) = context.as_ref() else { return };
            let Some(view) = view.upgrade() else { return };
            match super::column_select::projected_rows(&view, &selection, &columns) {
                super::column_select::Projection::Rows(picked, rows) => {
                    sender.send(GridMsg::CopyToClipboard(render(&picked, &rows))).ok();
                    return;
                }
                super::column_select::Projection::Incomplete => {
                    sender.send(GridMsg::IncompleteRowData).ok();
                    return;
                }
                super::column_select::Projection::NoSelection => {}
            }
            let Some(rows) = rows_for_menu(&view, position(slot)) else {
                sender.send(GridMsg::IncompleteRowData).ok();
                return;
            };
            sender.send(GridMsg::CopyToClipboard(render(&columns, &rows))).ok();
        })
        .build()
}

pub(super) struct CellGestureOptions {
    idx: usize,
    column_name: String,
    is_editable: bool,
    is_text_editable: bool,
    supports_full_value: bool,
}

pub(super) fn cell_menu_options(menus: &GridMenus, widget: &gtk::Widget) -> Option<CellGestureOptions> {
    let idx = super::display::COLUMN_SLOT.get(widget)?;
    let column = menus.columns.get(idx)?;
    let is_text_editable = widget.is::<crate::ui::cell_editor::CellEditor>();
    let is_editable = is_text_editable || widget.is::<gtk::CheckButton>();
    Some(CellGestureOptions {
        idx,
        column_name: column.name.clone(),
        is_editable,
        is_text_editable,
        supports_full_value: is_editable && super::full_value_editor::is_full_value_column(&column.data_type),
    })
}

pub(super) fn is_cell_menu_widget(menus: &GridMenus, widget: &gtk::Widget) -> bool {
    super::display::COLUMN_SLOT
        .get(widget)
        .is_some_and(|idx| menus.columns.get(idx).is_some())
}

fn prepare_cell_menu(
    menus: &GridMenus,
    view: &gtk::ColumnView,
    widget: &gtk::Widget,
    options: &CellGestureOptions,
) -> gtk::PopoverMenu {
    let position = POSITION_SLOT.get(widget).unwrap_or(0);
    select_row_for_menu(view, position);
    let row = row_object_at(view, position);
    let draft_target = row.as_ref().and_then(|row| row.draft_id()).map(|id| (position, id));
    menus.context.set(
        widget,
        options.idx,
        options.column_name.clone(),
        draft_target,
        row.as_ref(),
    );
    menus
        .edit_action
        .set_enabled(edit_enabled_for(widget, options.is_text_editable));
    if let Some((action, local_result_view)) = &menus.export_loaded {
        action.set_enabled(local_result_view.has_filter());
    }
    menus
        .edit_full_value_action
        .set_enabled(options.supports_full_value && row_has_full_text_value(view, widget, position, options.idx));
    menus.reference_action.set_enabled(
        menus.foreign_key_columns.contains(&options.column_name)
            && row_object_at(view, position).is_some_and(|row| {
                row.with_cell_value(options.idx, |value| !matches!(value, Value::Null))
                    .unwrap_or(false)
            }),
    );
    menus.show_referencing_action.set_enabled(
        menus.has_inbound_foreign_keys && row_object_at(view, position).is_some_and(|row| row.draft_id().is_none()),
    );
    if options.is_editable {
        menus.editable_popover.clone()
    } else {
        menus.readonly_popover.clone()
    }
}

fn edit_enabled_for(widget: &gtk::Widget, fallback: bool) -> bool {
    widget
        .downcast_ref::<crate::ui::cell_editor::CellEditor>()
        .map(|editor| editor.is_inline_editable())
        .unwrap_or(fallback)
}

fn position(slot: &CellContext) -> u32 {
    POSITION_SLOT.get(&slot.widget).unwrap_or(0)
}

fn menu_target_matches(view: &gtk::ColumnView, slot: &CellContext) -> bool {
    let Some(position) = slot.row_position else {
        return false;
    };
    slot.row.is_some()
        && POSITION_SLOT.get(&slot.widget) == Some(position)
        && row_object_at(view, position).as_ref() == slot.row.as_ref()
}

enum TextSeed {
    Value(String, super::full_value_editor::FullValueKind),
    TooLarge,
    Unsupported,
}

fn full_value_seed(
    widget: &gtk::Widget,
    row: &crate::ui::row_object::RowObject,
    column: usize,
    data_type: &str,
) -> TextSeed {
    if IS_NULL_SLOT.get(widget).unwrap_or(false) {
        return super::full_value_editor::edit_kind(data_type, &Value::Null)
            .map(|kind| TextSeed::Value(String::new(), kind))
            .unwrap_or(TextSeed::Unsupported);
    }
    if let Some(size) = FULL_EDIT_TEXT_SLOT.with(widget, String::len) {
        if !super::full_value_editor::within_edit_budget(size) {
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
        Value::Text(text) if super::full_value_editor::within_edit_budget(text.len()) => {
            super::full_value_editor::edit_kind(data_type, value)
                .map(|kind| TextSeed::Value(text.clone(), kind))
                .unwrap_or(TextSeed::Unsupported)
        }
        Value::Text(_) => TextSeed::TooLarge,
        value => super::full_value_editor::edit_kind(data_type, value)
            .map(|kind| {
                let text = super::display::value_to_full_edit_text(value);
                if super::full_value_editor::within_edit_budget(text.len()) {
                    TextSeed::Value(text, kind)
                } else {
                    TextSeed::TooLarge
                }
            })
            .unwrap_or(TextSeed::Unsupported),
    })
    .unwrap_or(TextSeed::Unsupported)
}

fn row_has_full_text_value(view: &gtk::ColumnView, widget: &gtk::Widget, position: u32, column: usize) -> bool {
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

fn has_persisted_row_key(row_key: &[Value]) -> bool {
    crate::services::change_tracker::RowKey::from_pk_values(row_key).is_some()
}

fn full_value_target(
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

fn cell_row_identity(widget: &gtk::Widget) -> (u32, Vec<Value>) {
    (
        POSITION_SLOT.get(widget).unwrap_or(0),
        ROW_KEY_SLOT.cloned(widget).map_or_else(Vec::new, |key| key.to_vec()),
    )
}
fn pending_cell_value(widget: &gtk::Widget) -> Option<Value> {
    if IS_NULL_SLOT.get(widget).unwrap_or(false) {
        return Some(Value::Null);
    }
    FULL_EDIT_TEXT_SLOT.cloned(widget).map(Value::Text)
}
fn copy_value_text(widget: &gtk::Widget, row: Option<&crate::ui::row_object::RowObject>, col: usize) -> String {
    if !IS_NULL_SLOT.get(widget).unwrap_or(false) {
        if let Some(text) = FULL_EDIT_TEXT_SLOT.cloned(widget) {
            return text;
        }
        if let Some(text) = row
            .and_then(|row| row.cell_value_in_row(col))
            .as_ref()
            .map(super::display::value_to_copy_text)
        {
            return text;
        }
    }
    cell_text(widget)
}

fn cell_text(widget: &gtk::Widget) -> String {
    if let Some(label) = widget.downcast_ref::<crate::ui::cell_editor::CellEditor>() {
        label.text().to_string()
    } else if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        label.text().to_string()
    } else {
        String::new()
    }
}
fn row_at(view: &gtk::ColumnView, position: u32) -> Option<Vec<Value>> {
    row_object_at(view, position).map(|row| row.cells_clone())
}
fn row_object_at(view: &gtk::ColumnView, position: u32) -> Option<crate::ui::row_object::RowObject> {
    view.model()?
        .item(position)?
        .downcast::<crate::ui::row_object::RowObject>()
        .ok()
}
fn complete_row_at(view: &gtk::ColumnView, position: u32) -> Option<Vec<Value>> {
    let row = view
        .model()?
        .item(position)?
        .downcast::<crate::ui::row_object::RowObject>()
        .ok()?;
    row.complete_cells()
}
fn rows_for_menu(view: &gtk::ColumnView, clicked: u32) -> Option<Vec<Vec<Value>>> {
    complete_rows(
        selected_row_positions(view, clicked)
            .into_iter()
            .map(|position| complete_row_at(view, position)),
    )
}
fn values_for_column_menu(view: &gtk::ColumnView, clicked: u32, column: usize) -> Option<Vec<Vec<Value>>> {
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
pub(super) fn selected_row_positions(view: &gtk::ColumnView, clicked: u32) -> Vec<u32> {
    view.model()
        .and_then(|model| model.downcast::<gtk::MultiSelection>().ok())
        .map(|selection| selected_positions(&selection))
        .filter(|positions| !positions.is_empty())
        .unwrap_or_else(|| vec![clicked])
}
fn complete_rows(rows: impl Iterator<Item = Option<Vec<Value>>>) -> Option<Vec<Vec<Value>>> {
    rows.collect()
}
fn selected_positions(selection: &gtk::MultiSelection) -> Vec<u32> {
    let bitset = selection.selection();
    (0..bitset.size()).map(|index| bitset.nth(index as u32)).collect()
}
fn select_row_for_menu(view: &gtk::ColumnView, position: u32) {
    if let Some(selection) = view
        .model()
        .and_then(|model| model.downcast::<gtk::MultiSelection>().ok())
        && position < selection.n_items()
        && !selection.is_selected(position)
    {
        selection.select_item(position, true);
    }
}
fn export_snapshot(view: &gtk::ColumnView, columns: &[ColumnInfo], truncated: bool) -> Option<QueryResult> {
    Some(QueryResult {
        columns: columns.to_vec(),
        rows: complete_rows((0..view.model()?.n_items()).map(|position| complete_row_at(view, position)))?.into(),
        truncated,
    })
}

#[cfg(test)]
#[path = "context_menu_tests.rs"]
mod tests;

pub(crate) struct LoadedCellEdit<'a> {
    pub(crate) column_name: &'a str,
    pub(crate) col_index: usize,
    pub(crate) row_position: u32,
    pub(crate) row_key: Vec<Value>,
    pub(crate) column: &'a bookie_core::ColumnInfo,
    pub(crate) value: Value,
    pub(crate) sender: relm4::Sender<GridMsg>,
}

pub(crate) fn present_loaded_cell_editor(view: &gtk::ColumnView, edit: LoadedCellEdit<'_>) {
    let Some(row) = row_object_at(view, edit.row_position) else {
        return;
    };
    let Some(kind) = super::full_value_editor::edit_kind(&edit.column.data_type, &edit.value) else {
        return;
    };
    let text = super::display::value_to_full_edit_text(&edit.value);
    if !super::full_value_editor::within_edit_budget(text.len()) {
        super::full_value_editor::present_too_large(view);
        return;
    }
    row.set_cell(edit.col_index, edit.value);
    super::full_value_editor::present(
        view,
        edit.column_name,
        &text,
        super::full_value_editor::EditContext {
            row_position: edit.row_position,
            col_index: edit.col_index,
            row_key: edit.row_key,
            draft_id: row.draft_id(),
            nullable: edit.column.nullable,
            kind,
            sender: edit.sender,
        },
    );
}
