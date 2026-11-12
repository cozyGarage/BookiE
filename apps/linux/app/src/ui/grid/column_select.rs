use std::cell::RefCell;
use std::rc::Rc;

use bookie_app_services::column_selection::{ColumnSelection, pick_columns};
use bookie_core::{ColumnInfo, Value};
use gtk4::{self as gtk, gio, prelude::*};

use super::cell_context::CellContextStore;
use super::display::COLUMN_SLOT;
use super::{GridMsg, SetCellsRequest, SetCellsRows};

pub type ColumnSelectionHandle = Rc<RefCell<ColumnSelection>>;

const CELL_CLASS: &str = "tp-column-selected";
const HEADER_CLASS: &str = "tp-column-header-selected";

pub(super) enum Projection {
    NoSelection,
    Incomplete,
    Rows(Vec<ColumnInfo>, Vec<Vec<Value>>),
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
    let selected = handle.borrow().columns();
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

fn first_label(root: &gtk::Widget) -> Option<gtk::Label> {
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
            return Some(label);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            stack.push(current);
        }
    }
    None
}

fn selected_titles(view: &gtk::ColumnView, selection: &ColumnSelection) -> Vec<String> {
    let model = view.columns();
    (0..model.n_items())
        .filter_map(|position| model.item(position)?.downcast::<gtk::ColumnViewColumn>().ok())
        .filter(|column| {
            column
                .id()
                .and_then(|id| id.parse::<usize>().ok())
                .is_some_and(|index| selection.contains(index))
        })
        .filter_map(|column| column.title().map(|title| title.to_string()))
        .collect()
}

fn refresh_headers(view: &gtk::ColumnView, selection: &ColumnSelection) {
    let titles = selected_titles(view, selection);
    let mut child = view.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.css_name() != "header" {
            continue;
        }
        let mut title = widget.first_child();
        while let Some(cell) = title {
            title = cell.next_sibling();
            let selected = first_label(&cell).is_some_and(|label| titles.iter().any(|text| *text == label.text()));
            set_class(&cell, HEADER_CLASS, selected);
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

fn changed(view: &gtk::ColumnView, handle: &ColumnSelectionHandle, sender: &Option<relm4::Sender<GridMsg>>) {
    refresh(view, handle);
    if let Some(sender) = sender {
        sender.send(GridMsg::ColumnSelectionChanged).ok();
    }
}

pub(crate) fn clear(view: &gtk::ColumnView, handle: &ColumnSelectionHandle, sender: &Option<relm4::Sender<GridMsg>>) {
    if handle.borrow().is_empty() {
        return;
    }
    handle.borrow_mut().clear();
    changed(view, handle, sender);
}

type Edit = fn(&mut ColumnSelection, usize);

fn selection_action(
    name: &str,
    view: &gtk::ColumnView,
    handle: &ColumnSelectionHandle,
    sender: &Option<relm4::Sender<GridMsg>>,
    edit: Edit,
) -> gio::SimpleAction {
    let action = gio::SimpleAction::new(name, Some(glib::VariantTy::UINT32));
    let view = view.downgrade();
    let handle = handle.clone();
    let sender = sender.clone();
    action.connect_activate(move |_, parameter| {
        let Some(column) = parameter.and_then(|value| value.get::<u32>()) else {
            return;
        };
        let Some(view) = view.upgrade() else { return };
        edit(&mut handle.borrow_mut(), column as usize);
        changed(&view, &handle, &sender);
    });
    action
}

fn set_column_action(handle: &ColumnSelectionHandle, sender: &relm4::Sender<GridMsg>) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("set-column-value", Some(glib::VariantTy::UINT32));
    let handle = handle.clone();
    let sender = sender.clone();
    action.connect_activate(move |_, parameter| {
        let Some(column) = parameter.and_then(|value| value.get::<u32>()) else {
            return;
        };
        let columns = target_columns(&handle.borrow(), column as usize);
        sender
            .send(GridMsg::SetCells(SetCellsRequest {
                columns,
                rows: SetCellsRows::AllLoaded,
            }))
            .ok();
    });
    action
}

pub(super) fn target_columns(selection: &ColumnSelection, clicked: usize) -> Vec<usize> {
    if selection.contains(clicked) {
        selection.columns()
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
) {
    actions.add_action(&selection_action(
        "select-column",
        view,
        handle,
        &sender,
        ColumnSelection::select_only,
    ));
    actions.add_action(&selection_action(
        "toggle-column",
        view,
        handle,
        &sender,
        ColumnSelection::toggle,
    ));
    actions.add_action(&selection_action(
        "extend-column",
        view,
        handle,
        &sender,
        ColumnSelection::extend_to,
    ));
    if let Some(sender) = sender.as_ref().filter(|_| set_cells) {
        actions.add_action(&set_column_action(handle, sender));
    }
    install_keys(view, handle, &sender);
    install_click_clear(view, handle, &sender);
}

fn install_keys(view: &gtk::ColumnView, handle: &ColumnSelectionHandle, sender: &Option<relm4::Sender<GridMsg>>) {
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = view.downgrade();
    let handle = handle.clone();
    let sender = sender.clone();
    controller.connect_key_pressed(move |_, key, _, state| {
        let Some(view) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let Some((_, column, _)) = super::display::focused_cell_identity(&view) else {
            return glib::Propagation::Proceed;
        };
        let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        if key == gtk::gdk::Key::space && ctrl && !state.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
            handle.borrow_mut().select_only(column);
            changed(&view, &handle, &sender);
            return glib::Propagation::Stop;
        }
        if key == gtk::gdk::Key::Escape && !handle.borrow().is_empty() {
            clear(&view, &handle, &sender);
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
) {
    let gesture = gtk::GestureClick::builder()
        .button(gtk::gdk::BUTTON_PRIMARY)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    let weak = view.downgrade();
    let handle = handle.clone();
    let sender = sender.clone();
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
                clear(&view, &handle, &sender);
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
            let columns = target_columns(&selection, slot.col_index);
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
        assert_eq!(target_columns(&selection, 3), vec![1, 3]);
        assert_eq!(target_columns(&selection, 2), vec![2]);
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
