//! `CellEditor` — the cell widget for editable ColumnView cells.
//!
//! A thin `Stack[Label | Text]` `gtk::Widget` subclass. The display
//! page is a plain `GtkLabel` that does not install any pointer-event
//! controllers, so a single click on the cell bubbles up to the
//! ColumnView's row-selection gesture without a forwarder. The edit
//! page is a `GtkText` shown only after `start_editing()`, where
//! click-to-position-cursor is the desired behaviour. Edit-mode
//! transitions are observable via `connect_editing_notify`, which
//! mirrors the stack's `visible-child-name` notify so callers can
//! snapshot the original text on entry and emit a commit on exit.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{glib, pango};

mod imp {
    use super::*;

    pub struct CellEditor {
        pub stack: gtk4::Stack,
        pub label: gtk4::Label,
        pub entry: RefCell<Option<gtk4::Text>>,
        pub preedit_callback: RefCell<Option<Rc<dyn Fn(&str)>>>,
        pub inline_editable: Cell<bool>,
    }

    impl Default for CellEditor {
        fn default() -> Self {
            Self {
                stack: gtk4::Stack::builder()
                    .transition_type(gtk4::StackTransitionType::None)
                    .hhomogeneous(true)
                    .vhomogeneous(true)
                    .build(),
                label: gtk4::Label::builder()
                    .xalign(0.0)
                    .hexpand(true)
                    .ellipsize(pango::EllipsizeMode::End)
                    .build(),
                entry: RefCell::new(None),
                preedit_callback: RefCell::new(None),
                inline_editable: Cell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CellEditor {
        const NAME: &str = "TableProCellEditor";
        type Type = super::CellEditor;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            // BinLayout sizes the single child (the stack) to fill
            // the cell — same effect as having a single-child
            // container.
            klass.set_layout_manager_type::<gtk4::BinLayout>();
        }
    }

    impl ObjectImpl for CellEditor {
        fn constructed(&self) {
            self.parent_constructed();
            // Stable CSS class so app-level selectors can target this
            // widget without depending on the type-name node default.
            self.obj().add_css_class("tp-cell-editor");
            let stack = self.stack.clone();
            let label = self.label.clone();
            stack.add_named(&label, Some("display"));
            stack.set_visible_child_name("display");
            stack.set_parent(&*self.obj());
        }

        fn dispose(&self) {
            // BinLayout-managed child must be unparented before our
            // destructor runs or GTK warns about a finalised widget
            // with leftover children.
            self.stack.unparent();
        }
    }

    impl WidgetImpl for CellEditor {}
}

glib::wrapper! {
    pub struct CellEditor(ObjectSubclass<imp::CellEditor>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn editable_cell_defers_its_text_widget_until_editing() {
        gtk4::init().unwrap();
        let cell = CellEditor::new();
        assert!(cell.stack_widget().child_by_name("edit").is_none());
        cell.set_text("stored");
        assert!(cell.stack_widget().child_by_name("edit").is_none());

        let preedit_seen = Rc::new(Cell::new(false));
        let preedit_seen_by_callback = preedit_seen.clone();
        cell.connect_preedit_changed(move |preedit| {
            preedit_seen_by_callback.set(preedit == "draft");
        });
        cell.set_inline_editable(true);
        cell.start_editing();
        let entry = cell.entry();
        entry.emit_by_name::<()>("preedit-changed", &[&"draft"]);
        assert!(preedit_seen.get());
        assert_eq!(entry.text().as_str(), "stored");
        entry.set_text("discarded");
        cell.stop_editing(false);
        assert_eq!(cell.text().as_str(), "stored");

        cell.start_editing_with("exact value");
        assert_eq!(cell.entry().text().as_str(), "exact value");
        cell.stop_editing(true);
        assert_eq!(cell.text().as_str(), "exact value");
        assert_eq!(cell.entry(), entry);
    }
}

impl Default for CellEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl CellEditor {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Current displayed text. In edit mode reads from the entry
    /// (so a still-editing cell returns the user's in-progress
    /// value); otherwise reads from the label.
    pub fn text(&self) -> glib::GString {
        if self.is_editing() {
            self.entry_widget().text()
        } else {
            self.label_widget().text()
        }
    }

    pub fn set_text(&self, s: &str) {
        self.label_widget().set_text(s);
        if let Some(entry) = self.imp().entry.borrow().as_ref() {
            entry.set_text(s);
        }
    }

    pub fn set_inline_editable(&self, editable: bool) {
        self.imp().inline_editable.set(editable);
        if !editable && self.is_editing() {
            self.stop_editing(false);
        }
    }

    pub fn is_inline_editable(&self) -> bool {
        self.imp().inline_editable.get()
    }

    /// Apply or clear a Pango strikethrough attribute on the display
    /// Label. Edit-mode `GtkText` is intentionally unaffected — a row
    /// marked for deletion is read-only by definition.
    pub fn set_strikethrough(&self, on: bool) {
        let label = self.label_widget();
        if on {
            let attrs = pango::AttrList::new();
            attrs.insert(pango::AttrInt::new_strikethrough(true));
            label.set_attributes(Some(&attrs));
        } else {
            label.set_attributes(None);
        }
    }

    pub fn is_editing(&self) -> bool {
        self.stack_widget()
            .visible_child_name()
            .map(|n| n == "edit")
            .unwrap_or(false)
    }

    pub fn start_editing(&self) {
        let seed = self.label_widget().text();
        self.start_editing_with(&seed);
    }

    /// Like `start_editing`, but seeds the entry from `seed` instead of
    /// the label. The label can hold a truncated display string for a
    /// long value, so a caller holding the untruncated text passes it
    /// here to avoid editing (and saving) the truncated copy.
    pub fn start_editing_with(&self, seed: &str) {
        if !self.is_inline_editable() {
            return;
        }
        let stack = self.stack_widget();
        let entry = self.entry_widget();
        entry.set_text(seed);
        stack.set_visible_child_name("edit");
        entry.grab_focus();
        // Select all so the first keystroke replaces — matches the
        // GTK Files rename flow and is what spreadsheet users expect.
        entry.select_region(0, -1);
    }

    pub fn stop_editing(&self, commit: bool) {
        let Some(entry) = self.imp().entry.borrow().as_ref().cloned() else {
            return;
        };
        if commit {
            let new_text = entry.text();
            self.label_widget().set_text(&new_text);
        } else {
            entry.set_text(&self.label_widget().text());
        }
        self.stack_widget().set_visible_child_name("display");
    }

    /// Subscribe to edit-mode toggles. The callback fires whenever
    /// the stack's visible child changes (display ↔ edit).
    pub fn connect_editing_notify<F: Fn(&Self) + 'static>(&self, callback: F) -> glib::SignalHandlerId {
        let stack = self.stack_widget();
        let weak = self.downgrade();
        stack.connect_visible_child_name_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                callback(&this);
            }
        })
    }

    /// The inner `GtkText` widget. Exposed so callers that need the
    /// IME-aware delegate (preedit-changed, etc.) can hook directly
    /// onto it.
    pub fn entry(&self) -> gtk4::Text {
        self.entry_widget()
    }

    pub fn connect_preedit_changed<F: Fn(&str) + 'static>(&self, callback: F) {
        let callback = Rc::new(callback);
        *self.imp().preedit_callback.borrow_mut() = Some(callback.clone());
        if let Some(entry) = self.imp().entry.borrow().as_ref() {
            entry.connect_preedit_changed(move |_, preedit| callback(preedit));
        }
    }

    fn stack_widget(&self) -> gtk4::Stack {
        self.imp().stack.clone()
    }

    fn label_widget(&self) -> gtk4::Label {
        self.imp().label.clone()
    }

    fn entry_widget(&self) -> gtk4::Text {
        if let Some(entry) = self.imp().entry.borrow().as_ref() {
            return entry.clone();
        }
        let entry = gtk4::Text::builder().hexpand(true).build();
        self.stack_widget().add_named(&entry, Some("edit"));
        self.install_entry_handlers(&entry);
        *self.imp().entry.borrow_mut() = Some(entry.clone());
        entry
    }

    fn install_entry_handlers(&self, entry: &gtk4::Text) {
        let weak_for_activate = self.downgrade();
        entry.connect_activate(move |_| {
            if let Some(this) = weak_for_activate.upgrade() {
                this.stop_editing(true);
            }
        });

        let key_ctrl = gtk4::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let weak_for_key = self.downgrade();
        key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk4::gdk::Key::Escape
                && let Some(this) = weak_for_key.upgrade()
                && this.is_editing()
            {
                this.stop_editing(false);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        entry.add_controller(key_ctrl);

        let focus_ctrl = gtk4::EventControllerFocus::new();
        let weak_for_focus = self.downgrade();
        focus_ctrl.connect_leave(move |_| {
            if let Some(this) = weak_for_focus.upgrade()
                && this.is_editing()
            {
                this.stop_editing(true);
            }
        });
        entry.add_controller(focus_ctrl);

        if let Some(callback) = self.imp().preedit_callback.borrow().as_ref().cloned() {
            entry.connect_preedit_changed(move |_, preedit| callback(preedit));
        }
    }
}
