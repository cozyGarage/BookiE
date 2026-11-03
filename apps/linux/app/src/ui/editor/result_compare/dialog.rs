use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;

use super::editor::CompareRequest;
use super::selection::{Candidate, key_choices};
use super::ticket::CompareTicket;
use crate::ui::editor::SqlEditorInput;

struct Form {
    dialog: gtk::glib::WeakRef<adw::Dialog>,
    candidates: Vec<Candidate>,
    left: gtk::DropDown,
    right: gtk::DropDown,
    keys: gtk::ListBox,
    note: gtk::Label,
    progress: gtk::Label,
    compare: gtk::Button,
    cancel: gtk::Button,
    ticket: RefCell<Option<Arc<CompareTicket>>>,
    sender: relm4::Sender<SqlEditorInput>,
}

fn dropdown(labels: &[String], selected: u32, name: &str) -> gtk::DropDown {
    let strings: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dropdown = gtk::DropDown::from_strings(&strings);
    dropdown.set_selected(selected);
    dropdown.set_hexpand(true);
    dropdown.set_widget_name(name);
    dropdown.update_property(&[gtk::accessible::Property::Label(name)]);
    dropdown
}

fn labelled(title: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.add_css_class("heading");
    row.append(&label);
    row.append(widget);
    row
}

fn note(text: String) -> gtk::Label {
    let label = gtk::Label::new(Some(&text));
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.add_css_class("dim-label");
    label
}

pub(in crate::ui::editor) fn present(
    parent: &impl IsA<gtk::Widget>,
    candidates: Vec<Candidate>,
    sender: relm4::Sender<SqlEditorInput>,
) {
    let labels: Vec<String> = candidates.iter().map(|candidate| candidate.label.clone()).collect();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    let dialog = adw::Dialog::builder()
        .title(crate::tr!("Compare results"))
        .content_width(520)
        .child(&content)
        .build();
    let form = Rc::new(Form {
        dialog: dialog.downgrade(),
        left: dropdown(&labels, 0, &crate::tr!("Left result")),
        right: dropdown(&labels, u32::from(labels.len() > 1), &crate::tr!("Right result")),
        keys: gtk::ListBox::new(),
        note: note(String::new()),
        progress: note(String::new()),
        compare: gtk::Button::with_label(&crate::tr!("Compare")),
        cancel: gtk::Button::with_label(&crate::tr!("Cancel")),
        candidates,
        ticket: RefCell::new(None),
        sender,
    });
    assemble(&form, &content);
    refresh_keys(&form);
    connect(&form);
    dialog.present(Some(parent));
}

fn assemble(form: &Rc<Form>, content: &gtk::Box) {
    content.append(&labelled(&crate::tr!("Left result"), &form.left));
    content.append(&labelled(&crate::tr!("Right result"), &form.right));
    form.keys.set_selection_mode(gtk::SelectionMode::None);
    form.keys.add_css_class("boxed-list");
    content.append(&labelled(&crate::tr!("Key columns (optional)"), &form.keys));
    content.append(&note(crate::tr!(
        "Without a key, whole rows are compared and row order is ignored. With a key, changed rows are shown as Before and After."
    )));
    content.append(&note(crate::tr!(
        "This compares the saved results only and records an entry in the audit journal. Values hidden by policy masking compare as the text shown."
    )));
    content.append(&form.note);
    content.append(&form.progress);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    form.compare.add_css_class("suggested-action");
    buttons.append(&form.cancel);
    buttons.append(&form.compare);
    content.append(&buttons);
}

fn refresh_keys(form: &Rc<Form>) {
    while let Some(row) = form.keys.first_child() {
        form.keys.remove(&row);
    }
    let (Some(left), Some(right)) = (selected(form, &form.left), selected(form, &form.right)) else {
        return;
    };
    match key_choices(left, right) {
        Ok(names) => {
            form.note.set_label("");
            for name in names {
                let check = gtk::CheckButton::with_label(&name);
                check.set_margin_start(8);
                check.set_margin_top(4);
                check.set_margin_bottom(4);
                form.keys.append(&check);
            }
        }
        Err(error) => form.note.set_label(&super::summary::failure_message(&error)),
    }
}

fn selected<'a>(form: &'a Form, dropdown: &gtk::DropDown) -> Option<&'a Candidate> {
    form.candidates.get(dropdown.selected() as usize)
}

fn chosen_keys(form: &Form) -> Vec<String> {
    let mut keys = Vec::new();
    let mut row = form.keys.first_child();
    while let Some(current) = row {
        if let Some(check) = current
            .first_child()
            .and_then(|child| child.downcast::<gtk::CheckButton>().ok())
            .filter(|check| check.is_active())
        {
            keys.extend(check.label().map(|label| label.to_string()));
        }
        row = current.next_sibling();
    }
    keys
}

fn build_request(form: &Form) -> Option<CompareRequest> {
    let left = selected(form, &form.left)?.clone();
    let right = selected(form, &form.right)?.clone();
    Some(CompareRequest::new(left, right, chosen_keys(form)))
}

fn connect(form: &Rc<Form>) {
    for dropdown in [&form.left, &form.right] {
        let weak = Rc::downgrade(form);
        dropdown.connect_selected_notify(move |_| {
            if let Some(form) = weak.upgrade() {
                refresh_keys(&form);
            }
        });
    }
    let weak = Rc::downgrade(form);
    form.compare.connect_clicked(move |_| {
        if let Some(form) = weak.upgrade() {
            start(&form);
        }
    });
    let weak = Rc::downgrade(form);
    form.cancel.connect_clicked(move |_| {
        let Some(form) = weak.upgrade() else { return };
        if !cancel_running(&form)
            && let Some(dialog) = form.dialog.upgrade()
        {
            dialog.force_close();
        }
    });
    if let Some(dialog) = form.dialog.upgrade() {
        let owner = form.clone();
        dialog.connect_closed(move |_| {
            cancel_running(&owner);
        });
    }
}

fn cancel_running(form: &Form) -> bool {
    let ticket = form.ticket.borrow();
    match ticket.as_ref().filter(|ticket| !ticket.is_finished()) {
        Some(ticket) => {
            ticket.cancel().cancel();
            true
        }
        None => false,
    }
}

fn start(form: &Rc<Form>) {
    if form.left.selected() == form.right.selected() {
        form.note.set_label(&crate::tr!("Choose two different results"));
        return;
    }
    let Some(request) = build_request(form) else { return };
    form.ticket.replace(Some(request.ticket.clone()));
    form.note.set_label("");
    for widget in [
        form.left.upcast_ref::<gtk::Widget>(),
        form.right.upcast_ref(),
        form.keys.upcast_ref(),
    ] {
        widget.set_sensitive(false);
    }
    form.compare.set_sensitive(false);
    form.progress.set_label(&crate::tr!("Comparing…"));
    let _ = form.sender.send(SqlEditorInput::CompareResults(Box::new(request)));
    let weak = Rc::downgrade(form);
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(150), move || match weak.upgrade() {
        Some(form) => poll(&form),
        None => gtk::glib::ControlFlow::Break,
    });
}

fn poll(form: &Rc<Form>) -> gtk::glib::ControlFlow {
    let Some(ticket) = form.ticket.borrow().clone() else {
        return gtk::glib::ControlFlow::Break;
    };
    if ticket.is_finished() {
        if let Some(dialog) = form.dialog.upgrade() {
            dialog.force_close();
        }
        return gtk::glib::ControlFlow::Break;
    }
    let rows = crate::ui::count_format::format_thousands(u128::from(ticket.progress().rows_done()));
    form.progress
        .set_label(&crate::tr!("Comparing… {rows} rows read").replace("{rows}", &rows));
    gtk::glib::ControlFlow::Continue
}
