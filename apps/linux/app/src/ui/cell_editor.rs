use glib::subclass::Signal;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{glib, pango};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;

type PreeditCallback = Rc<dyn Fn(&str)>;

mod imp {
    use super::*;

    pub struct CellEditor {
        pub label: gtk4::Label,
        pub entry: RefCell<Option<gtk4::Text>>,
        pub preedit_callback: RefCell<Option<PreeditCallback>>,
        pub last_preedit: RefCell<String>,
        pub inline_editable: Cell<bool>,
        pub editing: Cell<bool>,
    }

    impl Default for CellEditor {
        fn default() -> Self {
            Self {
                label: gtk4::Label::builder()
                    .xalign(0.0)
                    .hexpand(true)
                    .ellipsize(pango::EllipsizeMode::End)
                    .build(),
                entry: RefCell::new(None),
                preedit_callback: RefCell::new(None),
                last_preedit: RefCell::new(String::new()),
                inline_editable: Cell::default(),
                editing: Cell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CellEditor {
        const NAME: &str = "BookiECellEditor";
        type Type = super::CellEditor;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk4::BinLayout>();
        }
    }

    impl ObjectImpl for CellEditor {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().add_css_class("tp-cell-editor");
            self.label.set_parent(&*self.obj());
        }

        fn dispose(&self) {
            if let Some(entry) = self.entry.borrow().as_ref() {
                entry.unparent();
            }
            self.label.unparent();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("editing-changed").build()])
        }
    }

    impl WidgetImpl for CellEditor {}
}

glib::wrapper! {
    pub struct CellEditor(ObjectSubclass<imp::CellEditor>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
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
        self.imp().editing.get()
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
        let entry = self.entry_widget();
        entry.set_text(seed);
        if !self.imp().editing.replace(true) {
            self.label_widget().set_visible(false);
            entry.set_visible(true);
            self.emit_by_name::<()>("editing-changed", &[]);
        }
        entry.grab_focus();
        // Select all so the first keystroke replaces — matches the
        // GTK Files rename flow and is what spreadsheet users expect.
        entry.select_region(0, -1);
    }

    pub fn stop_editing(&self, commit: bool) {
        let Some(entry) = self.imp().entry.borrow().as_ref().cloned() else {
            return;
        };
        if !self.imp().editing.get() {
            return;
        }
        let pending = std::mem::take(&mut *self.imp().last_preedit.borrow_mut());
        if commit {
            let new_text = committed_edit_text(entry.text().as_str(), &pending);
            if new_text != entry.text().as_str() {
                entry.set_text(&new_text);
            }
            self.clear_preedit_listeners();
            self.label_widget().set_text(&new_text);
        } else {
            entry.set_text(&self.label_widget().text());
            self.clear_preedit_listeners();
        }
        entry.set_visible(false);
        self.label_widget().set_visible(true);
        self.imp().editing.set(false);
        self.emit_by_name::<()>("editing-changed", &[]);
    }

    pub fn connect_editing_notify<F: Fn(&Self) + 'static>(&self, callback: F) -> glib::SignalHandlerId {
        self.connect_local("editing-changed", false, move |values| {
            if let Some(this) = values.first().and_then(|value| value.get::<Self>().ok()) {
                callback(&this);
            }
            None
        })
    }

    /// The inner `GtkText` widget. Exposed so callers that need the
    /// IME-aware delegate (preedit-changed, etc.) can hook directly
    /// onto it.
    pub fn entry(&self) -> gtk4::Text {
        self.entry_widget()
    }

    pub fn connect_preedit_changed<F: Fn(&str) + 'static>(&self, callback: F) {
        *self.imp().preedit_callback.borrow_mut() = Some(Rc::new(callback));
    }

    fn record_preedit(&self, preedit: &str) {
        if !preedit.is_empty() {
            *self.imp().last_preedit.borrow_mut() = preedit.to_string();
        }
        if let Some(callback) = self.imp().preedit_callback.borrow().as_ref().cloned() {
            callback(preedit);
        }
    }

    fn clear_preedit_listeners(&self) {
        if let Some(callback) = self.imp().preedit_callback.borrow().as_ref().cloned() {
            callback("");
        }
    }

    fn label_widget(&self) -> gtk4::Label {
        self.imp().label.clone()
    }

    fn entry_widget(&self) -> gtk4::Text {
        if let Some(entry) = self.imp().entry.borrow().as_ref() {
            return entry.clone();
        }
        let entry = gtk4::Text::builder().hexpand(true).build();
        entry.set_visible(false);
        entry.set_parent(self);
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

        let weak_for_preedit = self.downgrade();
        entry.connect_preedit_changed(move |_, preedit| {
            if let Some(this) = weak_for_preedit.upgrade() {
                this.record_preedit(preedit);
            }
        });
    }
}

fn committed_edit_text(entry_text: &str, pending_preedit: &str) -> String {
    if pending_preedit.is_empty() || entry_text.ends_with(pending_preedit) {
        entry_text.to_string()
    } else {
        format!("{entry_text}{pending_preedit}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn editable_cell_defers_its_text_widget_until_editing() {
        gtk4::init().unwrap();
        let cell = CellEditor::new();
        assert!(cell.first_child().is_some_and(|child| child.is::<gtk4::Label>()));
        assert!(
            cell.imp()
                .label
                .parent()
                .is_some_and(|parent| parent == cell.clone().upcast::<gtk4::Widget>())
        );
        cell.set_text("stored");
        assert!(
            cell.imp()
                .label
                .parent()
                .is_some_and(|parent| parent == cell.clone().upcast::<gtk4::Widget>())
        );

        let preedit_seen = Rc::new(Cell::new(false));
        let preedit_seen_by_callback = preedit_seen.clone();
        cell.connect_preedit_changed(move |preedit| {
            preedit_seen_by_callback.set(preedit == "draft");
        });
        cell.set_inline_editable(true);
        let edit_transitions = Rc::new(Cell::new(0));
        let edit_transitions_callback = edit_transitions.clone();
        cell.connect_editing_notify(move |_| edit_transitions_callback.set(edit_transitions_callback.get() + 1));
        cell.start_editing();
        assert!(!cell.imp().label.is_visible());
        assert!(
            cell.imp()
                .entry
                .borrow()
                .as_ref()
                .is_some_and(|entry| entry.is_visible())
        );
        let entry = cell.entry();
        assert!(
            entry
                .parent()
                .is_some_and(|parent| parent == cell.clone().upcast::<gtk4::Widget>())
        );
        entry.emit_by_name::<()>("preedit-changed", &[&"draft"]);
        assert!(preedit_seen.get());
        assert_eq!(entry.text().as_str(), "stored");
        entry.set_text("discarded");
        cell.stop_editing(false);
        assert!(cell.imp().label.is_visible());
        assert!(!entry.property::<bool>("visible"));
        assert!(!entry.should_layout());
        assert!(
            entry
                .parent()
                .is_some_and(|parent| parent == cell.clone().upcast::<gtk4::Widget>())
        );
        assert_eq!(cell.text().as_str(), "stored");

        cell.start_editing_with("exact value");
        assert!(!cell.imp().label.is_visible());
        assert!(
            cell.imp()
                .entry
                .borrow()
                .as_ref()
                .is_some_and(|entry| entry.is_visible())
        );
        assert_eq!(cell.entry().text().as_str(), "exact value");
        cell.stop_editing(true);
        assert!(cell.imp().label.is_visible());
        assert!(!entry.property::<bool>("visible"));
        assert!(!entry.should_layout());
        assert_eq!(cell.text().as_str(), "exact value");
        assert_eq!(cell.entry(), entry);
        assert_eq!(edit_transitions.get(), 4);
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn focus_leave_commits_pending_preedit_text() {
        gtk4::init().unwrap();
        let cell = CellEditor::new();
        cell.set_inline_editable(true);
        cell.start_editing_with("Việt ");
        cell.entry().emit_by_name::<()>("preedit-changed", &[&"nam"]);
        cell.stop_editing(true);
        assert_eq!(cell.text().as_str(), "Việt nam");
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn committed_preedit_is_not_appended_twice() {
        gtk4::init().unwrap();
        let cell = CellEditor::new();
        cell.set_inline_editable(true);
        cell.start_editing_with("Việt nam");
        let entry = cell.entry();
        entry.emit_by_name::<()>("preedit-changed", &[&"nam"]);
        entry.emit_by_name::<()>("preedit-changed", &[&""]);
        cell.stop_editing(true);
        assert_eq!(cell.text().as_str(), "Việt nam");
    }

    #[test]
    fn pending_preedit_folds_into_committed_text() {
        assert_eq!(committed_edit_text("Việt ", "nam"), "Việt nam");
        assert_eq!(committed_edit_text("Việt nam", "nam"), "Việt nam");
        assert_eq!(committed_edit_text("Việt ", ""), "Việt ");
    }
}
