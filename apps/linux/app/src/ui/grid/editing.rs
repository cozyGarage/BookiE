use chrono::Datelike;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use sourceview5::prelude::*;

use super::GridMsg;
use super::display::{
    COLUMN_SLOT, EditSnapshot, FULL_EDIT_TEXT_SLOT, IS_NULL_SLOT, POPOVER_SLOT, POSITION_SLOT, PREEDIT_SLOT,
    ROW_KEY_SLOT, SNAPSHOT_SLOT, SUPPRESS_SLOT,
};
use super::types::CellEditorKind;
use crate::ui::cell_editor::CellEditor;

pub(super) fn enter_edit_mode(label: &CellEditor) {
    if !label.is_inline_editable() {
        return;
    }
    if IS_NULL_SLOT.get(label).unwrap_or(false) {
        label.start_editing_with("");
        return;
    }
    match FULL_EDIT_TEXT_SLOT.cloned(label) {
        Some(full) => label.start_editing_with(&full),
        None => label.start_editing(),
    }
}

pub(super) fn setup_editable_cell(item: &gtk::ListItem, idx: usize, sender: relm4::Sender<GridMsg>) {
    let label = CellEditor::new();
    label.set_hexpand(true);
    label.set_focusable(true);
    label.set_margin_start(8);
    label.set_margin_end(8);
    COLUMN_SLOT.set(&label, idx);
    item.set_child(Some(&label));

    install_edit_commit_handler(&label, idx, sender.clone());
}

pub(super) fn setup_bool_cell(item: &gtk::ListItem, idx: usize, sender: relm4::Sender<GridMsg>) {
    let checkbox = gtk::CheckButton::builder()
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .margin_start(8)
        .margin_end(8)
        .build();
    COLUMN_SLOT.set(&checkbox, idx);
    item.set_child(Some(&checkbox));

    checkbox.connect_toggled(move |cb| {
        if !cb.is_sensitive() || SUPPRESS_SLOT.get(cb).unwrap_or(false) {
            return;
        }
        let position = POSITION_SLOT.get(cb).unwrap_or(0);
        let row_key = ROW_KEY_SLOT.cloned(cb).map_or_else(Vec::new, |key| key.to_vec());
        let new_value = if cb.is_active() { "true" } else { "false" };
        sender
            .send(GridMsg::CellEdited {
                row_position: position,
                col_index: idx,
                new_value: new_value.to_string(),
                row_key,
            })
            .ok();
    });
}

pub(super) fn setup_readonly_cell(item: &gtk::ListItem, idx: usize, _sender: Option<relm4::Sender<GridMsg>>) {
    let label = gtk::Label::builder()
        .xalign(0.0)
        .hexpand(true)
        .selectable(true)
        .single_line_mode(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .margin_start(8)
        .margin_end(8)
        .build();
    COLUMN_SLOT.set(&label, idx);
    label.connect_map(sync_readonly_cell_focus);
    item.set_child(Some(&label));
}

pub(super) fn sync_readonly_cell_focus(label: &gtk::Label) {
    let Some(cell) = label
        .parent()
        .filter(|parent| parent.accessible_role() == gtk::AccessibleRole::GridCell)
    else {
        return;
    };
    if label.is_mapped() {
        cell.set_focusable(true);
    }
    if let Some(position) = POSITION_SLOT.get(label) {
        POSITION_SLOT.set(&cell, position);
    }
    if let Some(column) = COLUMN_SLOT.get(label) {
        COLUMN_SLOT.set(&cell, column);
    }
}

fn move_focus(widget: &impl IsA<gtk::Widget>, direction: gtk::DirectionType) {
    let Some(root) = widget.root() else { return };
    let Ok(window) = root.dynamic_cast::<gtk::Window>() else {
        return;
    };
    window.child_focus(direction);
}

pub(super) type CellEditTrigger = std::rc::Rc<dyn Fn(&CellEditor)>;

pub(super) fn build_edit_trigger(
    col_index: usize,
    sender: relm4::Sender<GridMsg>,
    editor_kind: CellEditorKind,
    column_name: std::rc::Rc<str>,
    choices: super::ChoiceMap,
) -> CellEditTrigger {
    let base: std::rc::Rc<dyn Fn(&CellEditor)> = match editor_kind {
        CellEditorKind::Text => std::rc::Rc::new(|l: &CellEditor| enter_edit_mode(l)),
        CellEditorKind::Date => {
            let sender = sender.clone();
            std::rc::Rc::new(move |l| show_calendar_popover(l, col_index, &sender))
        }
        CellEditorKind::Int => {
            let sender = sender.clone();
            std::rc::Rc::new(move |l| show_numeric_popover(l, col_index, &sender, false))
        }
        CellEditorKind::Float => {
            let sender = sender.clone();
            std::rc::Rc::new(move |l| show_numeric_popover(l, col_index, &sender, true))
        }
        CellEditorKind::Json => {
            let sender = sender.clone();
            std::rc::Rc::new(move |l| show_json_popover(l, col_index, &sender))
        }
    };
    let trigger: CellEditTrigger = {
        let sender = sender.clone();
        std::rc::Rc::new(move |l: &CellEditor| {
            let found = choices.borrow().get(column_name.as_ref()).cloned();
            match found {
                Some((is_set, labels)) => {
                    super::choice_editor::show_choice_popover(l, col_index, &sender, labels, is_set)
                }
                None => base(l),
            }
        })
    };
    std::rc::Rc::new(move |label: &CellEditor| {
        if label.is_inline_editable() {
            trigger(label);
        }
    })
}

pub(super) fn install_grid_edit_triggers(view: &impl IsA<gtk::Widget>, triggers: Vec<Option<CellEditTrigger>>) {
    if triggers.iter().all(Option::is_none) {
        return;
    }
    let view: gtk::Widget = view.clone().upcast();
    let weak_view = view.downgrade();
    let triggers = std::rc::Rc::new(triggers);

    let gesture = gtk::GestureClick::builder().button(gtk::gdk::BUTTON_PRIMARY).build();
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    let trigger_for_press = triggers.clone();
    let view_for_press = weak_view.clone();
    gesture.connect_pressed(move |gesture, n_press, x, y| {
        let Some(view) = view_for_press.upgrade() else {
            return;
        };
        let Some(target) = view.pick(x, y, gtk::PickFlags::DEFAULT) else {
            return;
        };
        let Some(cell) = cell_editor_ancestor(&target) else {
            return;
        };
        super::display::remember_clicked_cell(cell.upcast_ref());
        cell.grab_focus();
        if n_press != 2 {
            return;
        }
        let Some(trigger) = cell_edit_trigger(&cell, &trigger_for_press) else {
            return;
        };
        gesture.set_state(gtk::EventSequenceState::Claimed);
        trigger(&cell);
    });
    view.add_controller(gesture);

    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let trigger_for_key = triggers;
    let view_for_key = weak_view;
    controller.connect_key_pressed(move |_, keyval, _, modifiers| {
        let Some(view) = view_for_key.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let Some(cell) = focused_cell_editor(&view) else {
            return glib::Propagation::Proceed;
        };
        handle_cell_key(&cell, keyval, modifiers, &trigger_for_key)
    });
    view.add_controller(controller);
}

fn cell_edit_trigger<'a>(cell: &CellEditor, triggers: &'a [Option<CellEditTrigger>]) -> Option<&'a CellEditTrigger> {
    triggers.get(COLUMN_SLOT.get(cell)?)?.as_ref()
}

fn cell_editor_ancestor(widget: &gtk::Widget) -> Option<CellEditor> {
    if let Ok(cell) = widget.clone().downcast::<CellEditor>() {
        return Some(cell);
    }
    widget.parent().and_then(|parent| cell_editor_ancestor(&parent))
}

fn focused_cell_editor(view: &gtk::Widget) -> Option<CellEditor> {
    let root = view.root()?;
    let window = root.downcast::<gtk::Window>().ok()?;
    let focused = gtk::prelude::GtkWindowExt::focus(&window)?;
    cell_editor_ancestor(&focused)
}

fn handle_cell_key(
    cell: &CellEditor,
    keyval: gtk::gdk::Key,
    modifiers: gtk::gdk::ModifierType,
    triggers: &[Option<CellEditTrigger>],
) -> glib::Propagation {
    let editing = cell.is_editing();
    let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
    if !editing {
        return handle_readonly_cell_key(cell, keyval, shift, triggers);
    }
    match keyval {
        gtk::gdk::Key::Tab if !shift => {
            cell.stop_editing(true);
            move_focus(cell, gtk::DirectionType::TabForward);
            glib::Propagation::Stop
        }
        gtk::gdk::Key::Tab | gtk::gdk::Key::ISO_Left_Tab if shift => {
            cell.stop_editing(true);
            move_focus(cell, gtk::DirectionType::TabBackward);
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    }
}

fn handle_readonly_cell_key(
    cell: &CellEditor,
    keyval: gtk::gdk::Key,
    shift: bool,
    triggers: &[Option<CellEditTrigger>],
) -> glib::Propagation {
    match keyval {
        gtk::gdk::Key::F2 | gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
            if let Some(trigger) = cell_edit_trigger(cell, triggers) {
                trigger(cell);
                return glib::Propagation::Stop;
            }
        }
        gtk::gdk::Key::Tab if !shift => {
            move_focus(cell, gtk::DirectionType::TabForward);
            return glib::Propagation::Stop;
        }
        gtk::gdk::Key::Tab | gtk::gdk::Key::ISO_Left_Tab if shift => {
            move_focus(cell, gtk::DirectionType::TabBackward);
            return glib::Propagation::Stop;
        }
        gtk::gdk::Key::Right => {
            move_focus(cell, gtk::DirectionType::TabForward);
            return glib::Propagation::Stop;
        }
        gtk::gdk::Key::Left => {
            move_focus(cell, gtk::DirectionType::TabBackward);
            return glib::Propagation::Stop;
        }
        _ => {}
    }
    glib::Propagation::Proceed
}

fn show_calendar_popover(label: &CellEditor, col_index: usize, sender: &relm4::Sender<GridMsg>) {
    let calendar = gtk::Calendar::new();
    if let Ok(parsed) = chrono::NaiveDate::parse_from_str(label.text().as_str(), "%Y-%m-%d")
        && let Ok(dt) = glib::DateTime::from_local(parsed.year(), parsed.month() as i32, parsed.day() as i32, 0, 0, 0.0)
    {
        calendar.set_date(&dt);
    }

    let popover = gtk::Popover::builder().child(&calendar).build();
    popover.set_parent(label);
    POPOVER_SLOT.set(label, popover.clone());

    let label_for_cal = label.downgrade();
    let popover_for_cal = popover.downgrade();
    let sender_for_cal = sender.clone();
    calendar.connect_day_selected(move |c| {
        let Some(label_for_cal) = label_for_cal.upgrade() else {
            return;
        };
        let dt = c.date();
        let formatted = format!("{:04}-{:02}-{:02}", dt.year(), dt.month(), dt.day_of_month());
        commit_popover_value(&label_for_cal, col_index, &sender_for_cal, formatted);
        popdown_if_alive(&popover_for_cal);
    });

    install_popover_close_cleanup(label, &popover);
    popover.popup();
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum NumericEditor {
    Spin(f64),
    ExactText,
}

const MAX_EXACT_F64_INTEGER: i64 = 1 << 53;

pub(crate) fn numeric_editor(text: &str, is_float: bool) -> NumericEditor {
    let trimmed = text.trim();
    if is_float {
        return match trimmed.parse::<f64>() {
            Ok(value) if value.is_finite() => NumericEditor::Spin(value),
            _ => NumericEditor::ExactText,
        };
    }
    match trimmed.parse::<i64>() {
        Ok(value) if value.unsigned_abs() <= MAX_EXACT_F64_INTEGER as u64 => NumericEditor::Spin(value as f64),
        _ => NumericEditor::ExactText,
    }
}

fn show_numeric_popover(label: &CellEditor, col_index: usize, sender: &relm4::Sender<GridMsg>, is_float: bool) {
    match numeric_editor(label.text().as_str(), is_float) {
        NumericEditor::Spin(value) => show_spin_button_popover(label, col_index, sender, is_float, value),
        NumericEditor::ExactText => show_exact_text_popover(label, col_index, sender),
    }
}

fn show_exact_text_popover(label: &CellEditor, col_index: usize, sender: &relm4::Sender<GridMsg>) {
    let entry = gtk::Entry::new();
    entry.set_text(label.text().as_str());
    entry.set_width_chars(24);

    let popover = gtk::Popover::builder().child(&entry).build();
    popover.set_parent(label);
    POPOVER_SLOT.set(label, popover.clone());

    let label_for_commit = label.downgrade();
    let popover_for_commit = popover.downgrade();
    let sender_for_commit = sender.clone();
    entry.connect_activate(move |e| {
        let Some(label_for_commit) = label_for_commit.upgrade() else {
            return;
        };
        let formatted = e.text().to_string();
        commit_popover_value(&label_for_commit, col_index, &sender_for_commit, formatted);
        popdown_if_alive(&popover_for_commit);
    });

    install_popover_close_cleanup(label, &popover);
    popover.popup();
    entry.grab_focus();
}

fn show_spin_button_popover(
    label: &CellEditor,
    col_index: usize,
    sender: &relm4::Sender<GridMsg>,
    is_float: bool,
    initial: f64,
) {
    let (lower, upper, step, digits) = if is_float {
        (f64::MIN, f64::MAX, 0.1_f64, 6_u32)
    } else {
        let bound = MAX_EXACT_F64_INTEGER as f64;
        (-bound, bound, 1.0_f64, 0_u32)
    };
    let adjustment = gtk::Adjustment::new(initial, lower, upper, step, step * 10.0, 0.0);
    let spin = gtk::SpinButton::new(Some(&adjustment), step, digits);
    spin.set_numeric(true);
    spin.set_width_chars(20);

    let popover = gtk::Popover::builder().child(&spin).build();
    popover.set_parent(label);
    POPOVER_SLOT.set(label, popover.clone());

    let label_for_commit = label.downgrade();
    let popover_for_commit = popover.downgrade();
    let sender_for_commit = sender.clone();
    spin.connect_activate(move |s| {
        let Some(label_for_commit) = label_for_commit.upgrade() else {
            return;
        };
        let formatted = if is_float {
            format!("{}", s.value())
        } else {
            format!("{}", s.value() as i64)
        };
        commit_popover_value(&label_for_commit, col_index, &sender_for_commit, formatted);
        popdown_if_alive(&popover_for_commit);
    });

    install_popover_close_cleanup(label, &popover);
    popover.popup();
    spin.grab_focus();
}

fn show_json_popover(label: &CellEditor, col_index: usize, sender: &relm4::Sender<GridMsg>) {
    let buffer = sourceview5::Buffer::new(None);
    if let Some(lang) = sourceview5::LanguageManager::default().language("json") {
        buffer.set_language(Some(&lang));
    }
    let seed = FULL_EDIT_TEXT_SLOT
        .cloned(label)
        .unwrap_or_else(|| label.text().to_string());
    buffer.set_text(&seed);

    let view = sourceview5::View::with_buffer(&buffer);
    crate::ui::theme::style_editor(&view);
    view.set_show_line_numbers(true);
    view.set_monospace(true);
    view.set_auto_indent(true);
    view.set_tab_width(2);
    view.set_indent_width(2);

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_width(420)
        .min_content_height(280)
        .has_frame(true)
        .build();

    let save_button = gtk::Button::with_label(&crate::tr!("Save"));
    save_button.add_css_class("suggested-action");

    let cancel_button = gtk::Button::with_label(&crate::tr!("Cancel"));

    let button_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    button_box.append(&cancel_button);
    button_box.append(&save_button);

    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();
    container.append(&scrolled);
    container.append(&button_box);

    let popover = gtk::Popover::builder().child(&container).build();
    popover.set_parent(label);
    POPOVER_SLOT.set(label, popover.clone());

    let popover_for_cancel = popover.downgrade();
    cancel_button.connect_clicked(move |_| {
        popdown_if_alive(&popover_for_cancel);
    });

    let label_for_commit = label.downgrade();
    let popover_for_commit = popover.downgrade();
    let sender_for_commit = sender.clone();
    let buffer_for_commit = buffer.clone();
    save_button.connect_clicked(move |_| {
        let Some(label_for_commit) = label_for_commit.upgrade() else {
            return;
        };
        let start = buffer_for_commit.start_iter();
        let end = buffer_for_commit.end_iter();
        let text = buffer_for_commit.text(&start, &end, true).to_string();
        FULL_EDIT_TEXT_SLOT.set(&label_for_commit, text.clone());
        commit_popover_value(&label_for_commit, col_index, &sender_for_commit, text);
        popdown_if_alive(&popover_for_commit);
    });

    install_popover_close_cleanup(label, &popover);
    popover.popup();
    view.grab_focus();
}

fn install_popover_close_cleanup(label: &CellEditor, popover: &gtk::Popover) {
    let label_for_close = label.downgrade();
    popover.connect_closed(move |p| {
        p.unparent();
        if let Some(label_for_close) = label_for_close.upgrade() {
            POPOVER_SLOT.take(&label_for_close);
        }
    });
}

pub(super) fn commit_popover_value(
    label: &CellEditor,
    col_index: usize,
    sender: &relm4::Sender<GridMsg>,
    new_value: String,
) {
    let row_position = POSITION_SLOT.get(label).unwrap_or(0);
    let row_key = ROW_KEY_SLOT.cloned(label).map_or_else(Vec::new, |key| key.to_vec());
    label.set_text(&new_value);
    sender
        .send(GridMsg::CellEdited {
            row_position,
            col_index,
            new_value,
            row_key,
        })
        .ok();
}

fn popdown_if_alive(popover: &glib::WeakRef<gtk::Popover>) {
    if let Some(popover) = popover.upgrade() {
        popover.popdown();
    }
}

fn install_edit_commit_handler(label: &CellEditor, col_index: usize, sender: relm4::Sender<GridMsg>) {
    {
        let label_for_preedit = label.downgrade();
        let sender_for_preedit = sender.clone();
        label.connect_preedit_changed(move |preedit| {
            let Some(label_for_preedit) = label_for_preedit.upgrade() else {
                return;
            };
            let active = !preedit.is_empty();
            PREEDIT_SLOT.set(&label_for_preedit, active);
            if !active && !label_for_preedit.is_editing() {
                commit_cell_edit(&label_for_preedit, col_index, &sender_for_preedit);
            }
        });
    }

    label.connect_editing_notify(move |label| {
        if label.is_editing() {
            let position = POSITION_SLOT.get(label).unwrap_or(0);
            let original = FULL_EDIT_TEXT_SLOT
                .cloned(label)
                .unwrap_or_else(|| label.text().to_string());
            let row_key = ROW_KEY_SLOT.cloned(label).map_or_else(Vec::new, |key| key.to_vec());
            SNAPSHOT_SLOT.set(
                label,
                EditSnapshot {
                    position,
                    original,
                    row_key,
                },
            );
            return;
        }
        if PREEDIT_SLOT.get(label).unwrap_or(false) {
            return;
        }
        commit_cell_edit(label, col_index, &sender);
    });
}

#[cfg(test)]
mod cell_lifetime_tests {
    use std::time::{Duration, Instant};

    use super::*;
    use futures::FutureExt;

    fn drain() {
        let context = gtk::glib::MainContext::default();
        let until = Instant::now() + Duration::from_millis(100);
        while Instant::now() < until {
            while context.iteration(false) {}
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn assert_popover_released(open: impl FnOnce(&CellEditor, &relm4::Sender<GridMsg>)) {
        let cell = CellEditor::new();
        cell.set_text("2026-01-02");
        let cell_weak = cell.downgrade();
        let window = gtk::Window::builder().child(&cell).build();
        window.present();
        drain();
        let (sender, _receiver) = relm4::channel();
        open(&cell, &sender);
        let popover = POPOVER_SLOT.cloned(&cell).expect("editor installs its popover");
        let popover_weak = popover.downgrade();
        popover.popdown();
        drain();
        window.set_child(None::<&gtk::Widget>);
        window.destroy();
        drop(window);
        drop(popover);
        drop(cell);
        drain();
        assert!(cell_weak.upgrade().is_none(), "the retired editor is still alive");
        assert!(popover_weak.upgrade().is_none(), "the closed popover is still alive");
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn every_grid_edit_popover_releases_its_cell_and_widget() {
        gtk::init().unwrap();
        let cell = CellEditor::new();
        POSITION_SLOT.set(&cell, 5);
        ROW_KEY_SLOT.set(&cell, std::sync::Arc::from([bookie_core::Value::Int(23)]));
        let (sender, receiver) = relm4::channel();
        commit_popover_value(&cell, 2, &sender, "committed".into());
        assert!(matches!(
            receiver.recv().now_or_never().flatten(),
            Some(super::super::GridMsg::CellEdited {
                row_position: 5,
                col_index: 2,
                new_value,
                row_key,
            }) if new_value == "committed" && row_key == vec![bookie_core::Value::Int(23)]
        ));
        assert_popover_released(|cell, sender| show_calendar_popover(cell, 0, sender));
        assert_popover_released(|cell, sender| show_exact_text_popover(cell, 0, sender));
        assert_popover_released(|cell, sender| show_spin_button_popover(cell, 0, sender, false, 1.0));
        assert_popover_released(|cell, sender| show_json_popover(cell, 0, sender));
        assert_popover_released(|cell, sender| {
            super::super::choice_editor::show_choice_popover(cell, 0, sender, vec!["x".into()], false);
        });
    }
}

fn commit_cell_edit(label: &CellEditor, col_index: usize, sender: &relm4::Sender<GridMsg>) {
    let Some(snap) = SNAPSHOT_SLOT.take(label) else {
        return;
    };
    let new_value = label.text().to_string();
    if new_value == snap.original {
        return;
    }
    FULL_EDIT_TEXT_SLOT.set(label, new_value.clone());
    sender
        .send(GridMsg::CellEdited {
            row_position: snap.position,
            col_index,
            new_value,
            row_key: snap.row_key,
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::{NumericEditor, numeric_editor};

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn readonly_multiline_cells_keep_compact_height_and_exact_text() {
        use gtk4::prelude::*;
        gtk4::init().unwrap();
        let item = gtk4::glib::Object::new::<gtk4::ListItem>();
        super::setup_readonly_cell(&item, 0, None);
        let label = item.child().unwrap().downcast::<gtk4::Label>().unwrap();
        label.set_text("é東京,\"");
        let compact = label.measure(gtk4::Orientation::Vertical, 320).1;
        assert!(compact > 0);
        for text in [
            "é東京,\"\n".repeat(85),
            "a\rb\r\nc".into(),
            "a\u{2028}b\u{2029}c".into(),
            "short".into(),
        ] {
            label.set_text(&text);
            let height = label.measure(gtk4::Orientation::Vertical, 320).1;
            assert!(
                height <= compact * 2,
                "read-only preview height {height} exceeds compact height {compact}"
            );
            assert_eq!(label.text().as_str(), text);
        }
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn grid_edit_routing_commits_edits_after_double_click_f2_and_tab() {
        use futures::FutureExt;
        use gtk4::prelude::*;
        gtk4::init().unwrap();
        let readonly_view = gtk4::Fixed::new();
        super::install_grid_edit_triggers(&readonly_view, vec![None]);
        assert_eq!(readonly_view.observe_controllers().n_items(), 0);
        let view = gtk4::Fixed::new();
        view.set_size_request(240, 48);
        let editor = crate::ui::cell_editor::CellEditor::new();
        editor.set_size_request(120, 48);
        editor.set_focusable(true);
        editor.set_inline_editable(true);
        super::super::display::COLUMN_SLOT.set(&editor, 0);
        super::super::display::POSITION_SLOT.set(&editor, 0);
        let (sender, receiver) = relm4::channel();
        view.put(&editor, 0.0, 0.0);
        let second = crate::ui::cell_editor::CellEditor::new();
        second.set_size_request(120, 48);
        second.set_focusable(true);
        second.set_inline_editable(true);
        second.set_text("before");
        super::super::display::COLUMN_SLOT.set(&second, 1);
        super::super::display::POSITION_SLOT.set(&second, 0);
        super::install_edit_commit_handler(&second, 1, sender);
        view.put(&second, 120.0, 0.0);
        let window = gtk4::Window::builder()
            .child(&view)
            .default_width(240)
            .default_height(48)
            .build();
        window.present();
        for _ in 0..20 {
            while gtk4::glib::MainContext::default().iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let trigger: super::CellEditTrigger = std::rc::Rc::new(super::enter_edit_mode);
        let no_op: super::CellEditTrigger = std::rc::Rc::new(|_| {});
        super::install_grid_edit_triggers(&view, vec![Some(no_op), Some(trigger)]);
        let controllers = view.observe_controllers();
        let gesture = (0..controllers.n_items())
            .filter_map(|index| controllers.item(index)?.downcast::<gtk4::GestureClick>().ok())
            .find(|gesture| gesture.button() == gtk4::gdk::BUTTON_PRIMARY)
            .unwrap();
        gesture.emit_by_name::<()>("pressed", &[&2_i32, &150.0_f64, &20.0_f64]);
        assert!(!editor.is_editing());
        assert!(second.is_editing());
        second.stop_editing(false);
        assert!(second.grab_focus());
        let key_controller = (0..controllers.n_items())
            .filter_map(|index| controllers.item(index)?.downcast::<gtk4::EventControllerKey>().ok())
            .next()
            .unwrap();
        let propagation = key_controller.emit_by_name::<bool>(
            "key-pressed",
            &[&gtk4::gdk::Key::F2, &0_u32, &gtk4::gdk::ModifierType::empty()],
        );
        assert!(propagation);
        assert!(second.is_editing());
        second.entry().set_text("after");
        let propagation = key_controller.emit_by_name::<bool>(
            "key-pressed",
            &[&gtk4::gdk::Key::Tab, &0_u32, &gtk4::gdk::ModifierType::empty()],
        );
        assert!(propagation);
        assert!(!second.is_editing());
        assert_eq!(second.text().as_str(), "after");
        assert!(matches!(
            receiver.recv().now_or_never().flatten(),
            Some(super::super::GridMsg::CellEdited {
                row_position: 0,
                col_index: 1,
                new_value,
                row_key,
            }) if new_value == "after" && row_key.is_empty()
        ));
        window.close();
    }

    #[test]
    fn small_integers_use_the_spin_button() {
        assert_eq!(numeric_editor("42", false), NumericEditor::Spin(42.0));
        assert_eq!(numeric_editor(" -7 ", false), NumericEditor::Spin(-7.0));
        assert_eq!(
            numeric_editor("9007199254740992", false),
            NumericEditor::Spin(9007199254740992.0)
        );
    }

    #[test]
    fn wide_integers_use_exact_text_editing() {
        assert_eq!(numeric_editor("9007199254740993", false), NumericEditor::ExactText);
        assert_eq!(numeric_editor("-9007199254740993", false), NumericEditor::ExactText);
        assert_eq!(numeric_editor("9223372036854775807", false), NumericEditor::ExactText);
        assert_eq!(
            numeric_editor("170141183460469231731687303715884105727", false),
            NumericEditor::ExactText
        );
    }

    #[test]
    fn unreadable_numbers_keep_their_text_instead_of_becoming_zero() {
        assert_eq!(numeric_editor("", false), NumericEditor::ExactText);
        assert_eq!(numeric_editor("NULL", false), NumericEditor::ExactText);
        assert_eq!(numeric_editor("1_000", false), NumericEditor::ExactText);
        assert_eq!(numeric_editor("", true), NumericEditor::ExactText);
        assert_eq!(numeric_editor("nan", true), NumericEditor::ExactText);
        assert_eq!(numeric_editor("inf", true), NumericEditor::ExactText);
    }

    #[test]
    fn floats_use_the_spin_button() {
        assert_eq!(numeric_editor("1.5", true), NumericEditor::Spin(1.5));
        assert_eq!(numeric_editor("-0.25", true), NumericEditor::Spin(-0.25));
    }
}

#[cfg(test)]
mod lifetime_tests;
