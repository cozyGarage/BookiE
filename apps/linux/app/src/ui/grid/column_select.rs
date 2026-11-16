use std::cell::RefCell;
use std::rc::Rc;

use bookie_app_services::column_selection::{ColumnSelection, SelectionCopyFormat, copy_selection, pick_columns};
use bookie_core::{ColumnInfo, Value};
use gtk4::{self as gtk, gio, prelude::*};

use super::cell_context::CellContextStore;
use super::display::COLUMN_SLOT;
use super::{GridMsg, SetCellsRequest, SetCellsRows};

pub type ColumnSelectionHandle = Rc<RefCell<ColumnSelection>>;

#[derive(Clone, Copy)]
pub(super) struct CopySource<'a> {
    pub(super) columns: &'a [ColumnInfo],
    pub(super) driver_id: &'a str,
}

const CELL_CLASS: &str = "tp-column-selected";
const HEADER_CLASS: &str = "tp-column-header-selected";

pub(super) enum Projection {
    NoSelection,
    Incomplete,
    Rows(Vec<ColumnInfo>, Vec<Vec<Value>>),
}

pub(crate) fn selection_chrome() -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .visible(false)
        .margin_start(8)
        .margin_end(8)
        .margin_top(4)
        .margin_bottom(4)
        .build();
    let status = gtk::Label::builder().hexpand(true).halign(gtk::Align::Start).build();
    status.set_accessible_role(gtk::AccessibleRole::Status);
    row.append(&status);
    row
}

pub(crate) fn project_rows(
    columns: &[usize],
    positions: impl Iterator<Item = u32>,
    row_at: impl Fn(u32) -> Option<crate::ui::row_object::RowObject>,
) -> Option<Vec<Vec<Value>>> {
    positions
        .map(|position| {
            let row = row_at(position)?;
            columns
                .iter()
                .map(|column| {
                    if !row.cell_is_loaded(*column) {
                        return None;
                    }
                    row.with_cell_value(*column, Clone::clone)
                })
                .collect()
        })
        .collect()
}

pub(super) fn projected_rows(
    view: &gtk::ColumnView,
    handle: &ColumnSelectionHandle,
    columns: &[ColumnInfo],
) -> Projection {
    let selected = selection_columns(view, &handle.borrow());
    if selected.is_empty() {
        return Projection::NoSelection;
    }
    let Some(model) = view.model() else {
        return Projection::Incomplete;
    };
    let rows = project_rows(&selected, 0..model.n_items(), |position| {
        model
            .item(position)
            .and_then(|item| item.downcast::<crate::ui::row_object::RowObject>().ok())
    });
    match rows {
        Some(rows) => Projection::Rows(pick_columns(columns, &selected), rows),
        None => Projection::Incomplete,
    }
}

fn targeted_item(label: &str, action: &str, column: usize) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(label), None);
    item.set_action_and_target_value(Some(action), Some(&(column as u32).to_variant()));
    item
}

pub(super) fn header_section(column: usize, set_cells: bool) -> gio::Menu {
    let section = gio::Menu::new();
    section.append_item(&targeted_item(
        &bookie_app_services::tr!("Select Column"),
        "grid.select-column",
        column,
    ));
    section.append_item(&targeted_item(
        &bookie_app_services::tr!("Add Column to Selection"),
        "grid.toggle-column",
        column,
    ));
    section.append_item(&targeted_item(
        &bookie_app_services::tr!("Extend Selection to Column"),
        "grid.extend-column",
        column,
    ));
    if set_cells {
        section.append_item(&targeted_item(
            &bookie_app_services::tr!("Set Column to Value…"),
            "grid.set-column-value",
            column,
        ));
    }
    section
}

fn set_class(widget: &gtk::Widget, class: &str, on: bool) {
    if on {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

pub(super) fn apply_to_cell(child: &gtk::Widget, handle: &ColumnSelectionHandle, column: usize) {
    set_class(child, CELL_CLASS, handle.borrow().contains(column));
}

fn refresh_headers(view: &gtk::ColumnView, selection: &ColumnSelection) {
    let selected = selected_header_slots(&view_column_slots(view), selection);
    let mut child = view.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.css_name() != "header" {
            continue;
        }
        let mut title = widget.first_child();
        let mut position = 0;
        while let Some(cell) = title {
            title = cell.next_sibling();
            let selected = selected.get(position).copied().unwrap_or(false);
            position += 1;
            set_class(&cell, HEADER_CLASS, selected);
        }
    }
}

fn selected_header_slots(columns: &[Option<usize>], selection: &ColumnSelection) -> Vec<bool> {
    columns
        .iter()
        .map(|column| column.is_some_and(|index| selection.contains(index)))
        .collect()
}

fn view_column_slots(view: &gtk::ColumnView) -> Vec<Option<usize>> {
    let model = view.columns();
    (0..model.n_items())
        .filter_map(|position| model.item(position)?.downcast::<gtk::ColumnViewColumn>().ok())
        .filter(|column| column.is_visible())
        .map(|column| column.id().and_then(|id| id.parse().ok()))
        .collect()
}

fn view_columns(view: &gtk::ColumnView) -> Vec<usize> {
    view_column_slots(view).into_iter().flatten().collect()
}

fn selection_columns(view: &gtk::ColumnView, selection: &ColumnSelection) -> Vec<usize> {
    selection.columns_in_order(&view_columns(view))
}

fn update_chrome(view: &gtk::ColumnView, handle: &ColumnSelectionHandle, chrome: Option<&gtk::Box>) {
    let Some(chrome) = chrome else { return };
    let Some(label) = chrome
        .first_child()
        .and_then(|child| child.downcast::<gtk::Label>().ok())
    else {
        return;
    };
    let selected = selection_columns(view, &handle.borrow());
    let children: Vec<_> = std::iter::successors(chrome.first_child(), |widget| widget.next_sibling()).collect();
    for button in children
        .iter()
        .skip(1)
        .filter_map(|child| child.clone().downcast::<gtk::Button>().ok())
    {
        button.set_sensitive(!selected.is_empty());
    }
    if selected.is_empty() {
        chrome.set_visible(false);
        return;
    }
    let selected: std::collections::HashSet<_> = selected.into_iter().collect();
    let model = view.columns();
    let names: Vec<_> = (0..model.n_items())
        .filter_map(|position| model.item(position)?.downcast::<gtk::ColumnViewColumn>().ok())
        .filter(|column| column.is_visible())
        .filter(|column| {
            column
                .id()
                .and_then(|id| id.parse::<usize>().ok())
                .is_some_and(|id| selected.contains(&id))
        })
        .filter_map(|column| column.title().map(|title| title.to_string()))
        .collect();
    let rows = view.model().map_or(0, |model| model.n_items());
    label.set_label(
        &bookie_app_services::tr!("Selected columns: {columns}; loaded rows only: {rows}")
            .replace("{columns}", &names.join(", "))
            .replace("{rows}", &rows.to_string()),
    );
    chrome.set_visible(true);
}

fn install_chrome(
    view: &gtk::ColumnView,
    handle: &ColumnSelectionHandle,
    chrome: &gtk::Box,
    source: CopySource<'_>,
    sender: &Option<relm4::Sender<GridMsg>>,
) {
    let Some(sender) = sender else { return };
    let mut formats = vec![
        (bookie_app_services::tr!("Copy TSV"), SelectionCopyFormat::Tsv),
        (bookie_app_services::tr!("Copy CSV"), SelectionCopyFormat::Csv),
        (bookie_app_services::tr!("Copy JSON"), SelectionCopyFormat::Json),
    ];
    if bookie_core::export::supports_sql_literals(source.driver_id) {
        formats.push((bookie_app_services::tr!("Copy IN list"), SelectionCopyFormat::InList));
    }
    let columns: Rc<[ColumnInfo]> = Rc::from(source.columns);
    let driver_id = source.driver_id.to_owned();
    for (label, format) in formats {
        let button = gtk::Button::with_label(&label);
        button.set_sensitive(false);
        let view = view.downgrade();
        let handle = handle.clone();
        let columns = columns.clone();
        let driver_id = driver_id.clone();
        let sender = sender.clone();
        button.connect_clicked(move |_| {
            let Some(view) = view.upgrade() else { return };
            copy_selected(
                &view,
                &handle,
                CopySource {
                    columns: &columns,
                    driver_id: &driver_id,
                },
                format,
                &Some(sender.clone()),
            );
        });
        chrome.append(&button);
    }
}

fn copy_selected(
    view: &gtk::ColumnView,
    handle: &ColumnSelectionHandle,
    source: CopySource<'_>,
    format: SelectionCopyFormat,
    sender: &Option<relm4::Sender<GridMsg>>,
) {
    let Some(sender) = sender else { return };
    match projected_rows(view, handle, source.columns) {
        Projection::Rows(columns, rows) => match copy_selection(format, source.driver_id, &columns, &rows) {
            Ok(copy) => {
                sender.send(GridMsg::CopyToClipboard(copy.text)).ok();
                if copy.note.is_some() {
                    sender
                        .send(GridMsg::ShowToast(bookie_app_services::tr!(
                            "Some values were left out of the list."
                        )))
                        .ok();
                }
            }
            Err(_) => {
                sender
                    .send(GridMsg::ShowToast(bookie_app_services::tr!(
                        "These values cannot be copied in this format."
                    )))
                    .ok();
            }
        },
        Projection::Incomplete => {
            sender.send(GridMsg::IncompleteRowData).ok();
        }
        Projection::NoSelection => {
            sender
                .send(GridMsg::ShowToast(bookie_app_services::tr!("Select a column first")))
                .ok();
        }
    }
}

fn apply_cell_classes(root: &gtk::Widget, selection: &ColumnSelection) {
    let mut stack: Vec<gtk::Widget> = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            stack.push(current);
        }
        let Some(column) = COLUMN_SLOT.get(&widget) else {
            continue;
        };
        if widget.accessible_role() != gtk::AccessibleRole::GridCell {
            set_class(&widget, CELL_CLASS, selection.contains(column));
        }
    }
}

pub(crate) fn refresh(view: &gtk::ColumnView, handle: &ColumnSelectionHandle) {
    let selection = handle.borrow();
    apply_cell_classes(view.upcast_ref(), &selection);
    refresh_headers(view, &selection);
}

fn changed(
    view: &gtk::ColumnView,
    handle: &ColumnSelectionHandle,
    sender: &Option<relm4::Sender<GridMsg>>,
    chrome: Option<&gtk::Box>,
) {
    refresh(view, handle);
    update_chrome(view, handle, chrome);
    if let Some(sender) = sender {
        sender.send(GridMsg::ColumnSelectionChanged).ok();
    }
}

pub(crate) fn clear(
    view: &gtk::ColumnView,
    handle: &ColumnSelectionHandle,
    sender: &Option<relm4::Sender<GridMsg>>,
    chrome: Option<&gtk::Box>,
) {
    if handle.borrow().is_empty() {
        return;
    }
    handle.borrow_mut().clear();
    changed(view, handle, sender, chrome);
}

type Edit = fn(&mut ColumnSelection, usize);

fn selection_action(
    name: &str,
    view: &gtk::ColumnView,
    handle: &ColumnSelectionHandle,
    sender: &Option<relm4::Sender<GridMsg>>,
    edit: Edit,
    chrome: Option<&gtk::Box>,
) -> gio::SimpleAction {
    let extend = name == "extend-column";
    let action = gio::SimpleAction::new(name, Some(glib::VariantTy::UINT32));
    let view = view.downgrade();
    let handle = handle.clone();
    let sender = sender.clone();
    let chrome = chrome.cloned();
    action.connect_activate(move |_, parameter| {
        let Some(column) = parameter.and_then(|value| value.get::<u32>()) else {
            return;
        };
        let Some(view) = view.upgrade() else { return };
        if extend {
            handle
                .borrow_mut()
                .extend_to_in_order(column as usize, &view_columns(&view));
        } else {
            edit(&mut handle.borrow_mut(), column as usize);
        }
        changed(&view, &handle, &sender, chrome.as_ref());
    });
    action
}

fn set_column_action(
    view: &gtk::ColumnView,
    handle: &ColumnSelectionHandle,
    sender: &relm4::Sender<GridMsg>,
) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("set-column-value", Some(glib::VariantTy::UINT32));
    let view = view.downgrade();
    let handle = handle.clone();
    let sender = sender.clone();
    action.connect_activate(move |_, parameter| {
        let Some(column) = parameter.and_then(|value| value.get::<u32>()) else {
            return;
        };
        let Some(view) = view.upgrade() else { return };
        let columns = target_columns(&handle.borrow(), column as usize, &view_columns(&view));
        sender
            .send(GridMsg::SetCells(SetCellsRequest {
                columns,
                rows: SetCellsRows::AllLoaded,
            }))
            .ok();
    });
    action
}

pub(super) fn target_columns(selection: &ColumnSelection, clicked: usize, order: &[usize]) -> Vec<usize> {
    if selection.contains(clicked) {
        selection.columns_in_order(order)
    } else {
        vec![clicked]
    }
}

pub(super) fn install(
    view: &gtk::ColumnView,
    actions: &gio::SimpleActionGroup,
    handle: &ColumnSelectionHandle,
    sender: Option<relm4::Sender<GridMsg>>,
    set_cells: bool,
    source: CopySource<'_>,
    chrome: Option<&gtk::Box>,
) {
    actions.add_action(&selection_action(
        "select-column",
        view,
        handle,
        &sender,
        ColumnSelection::select_only,
        chrome,
    ));
    actions.add_action(&selection_action(
        "toggle-column",
        view,
        handle,
        &sender,
        ColumnSelection::toggle,
        chrome,
    ));
    actions.add_action(&selection_action(
        "extend-column",
        view,
        handle,
        &sender,
        ColumnSelection::extend_to,
        chrome,
    ));
    if let Some(sender) = sender.as_ref().filter(|_| set_cells) {
        actions.add_action(&set_column_action(view, handle, sender));
    }
    if let Some(chrome) = chrome {
        install_chrome(view, handle, chrome, source, &sender);
        update_chrome(view, handle, Some(chrome));
        let weak = view.downgrade();
        let handle = handle.clone();
        let chrome = chrome.clone();
        view.columns().connect_items_changed(move |_, _, _, _| {
            if let Some(view) = weak.upgrade() {
                refresh(&view, &handle);
                update_chrome(&view, &handle, Some(&chrome));
            }
        });
    }
    install_keys(view, handle, &sender, chrome, source);
    install_click_clear(view, handle, &sender, chrome);
}

fn install_keys(
    view: &gtk::ColumnView,
    handle: &ColumnSelectionHandle,
    sender: &Option<relm4::Sender<GridMsg>>,
    chrome: Option<&gtk::Box>,
    source: CopySource<'_>,
) {
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = view.downgrade();
    let handle = handle.clone();
    let sender = sender.clone();
    let chrome = chrome.cloned();
    let columns: Rc<[ColumnInfo]> = Rc::from(source.columns);
    let driver_id = source.driver_id.to_owned();
    controller.connect_key_pressed(move |_, key, _, state| {
        let Some(view) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        if key == gtk::gdk::Key::c
            && ctrl
            && !state.intersects(gtk::gdk::ModifierType::SHIFT_MASK | gtk::gdk::ModifierType::ALT_MASK)
            && !handle.borrow().is_empty()
        {
            copy_selected(
                &view,
                &handle,
                CopySource {
                    columns: &columns,
                    driver_id: &driver_id,
                },
                SelectionCopyFormat::Tsv,
                &sender,
            );
            return glib::Propagation::Stop;
        }
        let Some((_, column, _)) = super::display::focused_cell_identity(&view) else {
            return glib::Propagation::Proceed;
        };
        if key == gtk::gdk::Key::space && ctrl && !state.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
            handle.borrow_mut().select_only(column);
            changed(&view, &handle, &sender, chrome.as_ref());
            return glib::Propagation::Stop;
        }
        if key == gtk::gdk::Key::Escape && !handle.borrow().is_empty() {
            clear(&view, &handle, &sender, chrome.as_ref());
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    view.add_controller(controller);
}

fn install_click_clear(
    view: &gtk::ColumnView,
    handle: &ColumnSelectionHandle,
    sender: &Option<relm4::Sender<GridMsg>>,
    chrome: Option<&gtk::Box>,
) {
    let gesture = gtk::GestureClick::builder()
        .button(gtk::gdk::BUTTON_PRIMARY)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    let weak = view.downgrade();
    let handle = handle.clone();
    let sender = sender.clone();
    let chrome = chrome.cloned();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let Some(view) = weak.upgrade() else { return };
        let extended = gesture
            .current_event_state()
            .intersects(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK);
        if extended || handle.borrow().is_empty() {
            return;
        }
        let mut widget = view.pick(x, y, gtk::PickFlags::DEFAULT);
        while let Some(current) = widget {
            if current == view {
                return;
            }
            if COLUMN_SLOT.get(&current).is_some() {
                clear(&view, &handle, &sender, chrome.as_ref());
                return;
            }
            widget = current.parent();
        }
    });
    view.add_controller(gesture);
}

pub(super) fn set_cells_entry(
    context: CellContextStore,
    sender: relm4::Sender<GridMsg>,
    view: &gtk::ColumnView,
    handle: ColumnSelectionHandle,
) -> gio::ActionEntry<gio::SimpleActionGroup> {
    let view = view.downgrade();
    gio::ActionEntry::builder("set-cells")
        .activate(move |_, _, _| {
            let Some(slot) = context.snapshot() else { return };
            let Some(view) = view.upgrade() else { return };
            let selection = handle.borrow();
            let rows = if selection.contains(slot.col_index) {
                SetCellsRows::AllLoaded
            } else {
                SetCellsRows::Positions(super::context_menu::selected_row_positions(
                    &view,
                    slot.row_position.unwrap_or(0),
                ))
            };
            let columns = target_columns(&selection, slot.col_index, &view_columns(&view));
            sender.send(GridMsg::SetCells(SetCellsRequest { columns, rows })).ok();
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::row_object::RowObject;

    #[test]
    fn projecting_rows_keeps_only_the_selected_columns_in_order() {
        let rows = [
            RowObject::new(vec![Value::Int(1), Value::Text("a".into()), Value::Int(9)]),
            RowObject::new(vec![Value::Int(2), Value::Text("b".into()), Value::Int(8)]),
        ];
        let projected = project_rows(&[2, 0], 0..2, |position| rows.get(position as usize).cloned()).unwrap();
        assert_eq!(
            projected,
            vec![vec![Value::Int(9), Value::Int(1)], vec![Value::Int(8), Value::Int(2)]]
        );
    }

    #[test]
    fn projecting_fails_when_a_selected_cell_is_not_loaded() {
        let row = RowObject::new_projected(vec![Value::Int(7)], &[0], 2).unwrap();
        assert!(project_rows(&[1], 0..1, |_| Some(row.clone())).is_none());
        assert!(project_rows(&[0], 0..1, |_| Some(row.clone())).is_some());
    }

    #[test]
    fn a_clicked_column_outside_the_selection_is_the_only_target() {
        let mut selection = ColumnSelection::default();
        selection.select_only(1);
        selection.toggle(3);
        assert_eq!(target_columns(&selection, 3, &[3, 1, 2]), vec![3, 1]);
        assert_eq!(target_columns(&selection, 2, &[3, 1, 2]), vec![2]);
    }

    #[test]
    fn duplicate_header_titles_keep_distinct_column_identity() {
        let mut selection = ColumnSelection::default();
        selection.select_only(2);
        assert_eq!(
            selected_header_slots(&[None, Some(2), Some(0), Some(1)], &selection),
            [false, true, false, false]
        );
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn selected_column_cells_get_the_selection_style_and_lose_it_on_clear() {
        gtk::init().unwrap();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let cells: Vec<gtk::Label> = (0..3).map(|_| gtk::Label::new(None)).collect();
        for (index, cell) in cells.iter().enumerate() {
            COLUMN_SLOT.set(cell, index);
            root.append(cell);
        }
        let mut selection = ColumnSelection::default();
        selection.select_only(1);
        selection.toggle(2);
        apply_cell_classes(root.upcast_ref(), &selection);
        let styled: Vec<bool> = cells.iter().map(|cell| cell.has_css_class(CELL_CLASS)).collect();
        assert_eq!(styled, [false, true, true]);
        selection.clear();
        apply_cell_classes(root.upcast_ref(), &selection);
        assert!(cells.iter().all(|cell| !cell.has_css_class(CELL_CLASS)));
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_header_menu_offers_column_selection_and_only_offers_set_cells_when_enabled() {
        gtk::init().unwrap();
        let labels = |menu: &gio::Menu| -> Vec<String> {
            (0..menu.n_items())
                .filter_map(|index| menu.item_attribute_value(index, "label", None)?.get::<String>())
                .collect()
        };
        let read_only = labels(&header_section(0, false));
        assert!(read_only.contains(&"Select Column".to_owned()));
        assert!(!read_only.iter().any(|label| label.starts_with("Set Column")));
        assert!(
            labels(&header_section(0, true))
                .iter()
                .any(|label| label.starts_with("Set Column"))
        );
    }
}
