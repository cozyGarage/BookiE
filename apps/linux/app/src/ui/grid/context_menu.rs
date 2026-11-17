mod copy_actions;
mod full_value;
mod keyboard_menu;
mod menu_model;
mod pointer_menu;
mod row_access;
mod row_actions;
mod value_actions;
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

pub(super) use row_access::selected_row_positions;
use row_access::*;

use full_value::*;

use menu_model::*;

use value_actions::*;

use row_actions::*;

use copy_actions::*;

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
