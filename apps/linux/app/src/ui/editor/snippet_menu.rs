use std::cell::OnceCell;

use bookie_app_services::snippets::{self, Expansion, Snippet, SnippetStore};
use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};

use super::{SqlEditor, SqlEditorInput};

thread_local! {
    static STORE: OnceCell<Option<SnippetStore>> = const { OnceCell::new() };
}

fn store() -> Option<SnippetStore> {
    STORE.with(|cell| cell.get_or_init(SnippetStore::open).clone())
}

pub(super) fn build(sender: &ComponentSender<SqlEditor>) -> gtk::MenuButton {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    let popover = gtk::Popover::builder().child(&content).build();
    let menu = gtk::MenuButton::builder()
        .icon_name("insert-text-symbolic")
        .tooltip_text(bookie_app_services::tr!("Insert a saved SQL snippet"))
        .popover(&popover)
        .build();
    menu.add_css_class("flat");
    menu.update_property(&[gtk::accessible::Property::Label(&bookie_app_services::tr!("SQL snippets"))]);
    popover.connect_show({
        let (sender, content, popover) = (sender.clone(), content, popover.downgrade());
        move |_| fill(&content, &sender, &popover)
    });
    menu
}

fn fill(content: &gtk::Box, sender: &ComponentSender<SqlEditor>, popover: &gtk::glib::WeakRef<gtk::Popover>) {
    while let Some(child) = content.first_child() {
        content.remove(&child);
    }
    let Some(store) = store() else {
        content.append(&gtk::Label::new(Some(&bookie_app_services::tr!(
            "Snippets cannot be saved on this system."
        ))));
        return;
    };
    for snippet in store.list() {
        content.append(&snippet_row(&snippet, &store, sender, popover));
    }
    let new = gtk::Button::with_label(&bookie_app_services::tr!("New snippet…"));
    new.add_css_class("flat");
    new.connect_clicked({
        let (store, popover) = (store, popover.clone());
        move |button| {
            close(&popover);
            present_editor(button, &store, None);
        }
    });
    content.append(&new);
}

fn snippet_row(
    snippet: &Snippet,
    store: &SnippetStore,
    sender: &ComponentSender<SqlEditor>,
    popover: &gtk::glib::WeakRef<gtk::Popover>,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    let insert = gtk::Button::builder()
        .label(&snippet.name)
        .tooltip_text(snippet.body.lines().next().unwrap_or_default())
        .hexpand(true)
        .build();
    insert.add_css_class("flat");
    insert.connect_clicked({
        let (body, sender, popover) = (snippet.body.clone(), sender.clone(), popover.clone());
        move |_| {
            close(&popover);
            sender.input(SqlEditorInput::InsertSnippet(body.clone()));
        }
    });
    let edit = icon_button("document-edit-symbolic", &bookie_app_services::tr!("Edit snippet"));
    edit.connect_clicked({
        let (snippet, store, popover) = (snippet.clone(), store.clone(), popover.clone());
        move |button| {
            close(&popover);
            present_editor(button, &store, Some(&snippet));
        }
    });
    let delete = icon_button("user-trash-symbolic", &bookie_app_services::tr!("Delete snippet"));
    delete.connect_clicked({
        let (name, store, popover) = (snippet.name.clone(), store.clone(), popover.clone());
        move |button| {
            close(&popover);
            if let Err(message) = store.delete(&name).and_then(|_| store.flush()) {
                present_error(button, &message);
            }
        }
    });
    row.append(&insert);
    row.append(&edit);
    row.append(&delete);
    row
}

fn icon_button(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(label));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}

fn close(popover: &gtk::glib::WeakRef<gtk::Popover>) {
    if let Some(popover) = popover.upgrade() {
        popover.popdown();
    }
}

fn present_editor(parent: &gtk::Button, store: &SnippetStore, existing: Option<&Snippet>) {
    let name = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("Name"))
        .text(existing.map_or("", |snippet| snippet.name.as_str()))
        .build();
    let body = gtk::TextView::builder()
        .monospace(true)
        .accepts_tab(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    body.buffer()
        .set_text(existing.map_or("", |snippet| snippet.body.as_str()));
    let scroller = gtk::ScrolledWindow::builder()
        .child(&body)
        .min_content_height(140)
        .min_content_width(380)
        .build();
    let form = gtk::Box::new(gtk::Orientation::Vertical, 8);
    form.append(&name);
    form.append(&scroller);
    form.append(
        &gtk::Label::builder()
            .label(bookie_app_services::tr!(
                "Put $0 where the cursor should land. Write $$ for a literal $."
            ))
            .css_classes(["dim-label"])
            .xalign(0.0)
            .wrap(true)
            .build(),
    );
    let dialog = adw::AlertDialog::builder()
        .heading(bookie_app_services::tr!("SQL snippet"))
        .extra_child(&form)
        .build();
    dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
    dialog.add_response("save", &bookie_app_services::tr!("Save"));
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let old_name = existing.map(|snippet| snippet.name.clone());
    let (store, parent_weak) = (store.clone(), parent.downgrade());
    dialog.connect_response(Some("save"), move |_, _| {
        let buffer = body.buffer();
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        let outcome = save_snippet(&store, old_name.as_deref(), name.text().as_str(), text.as_str());
        if let (Err(message), Some(parent)) = (outcome, parent_weak.upgrade()) {
            present_error(&parent, &message);
        }
    });
    dialog.present(Some(parent));
}

fn save_snippet(store: &SnippetStore, old_name: Option<&str>, name: &str, body: &str) -> Result<(), String> {
    store.save(name, body)?;
    if let Some(old) = old_name.filter(|old| old.trim().to_lowercase() != name.trim().to_lowercase()) {
        store.delete(old)?;
    }
    store.flush()
}

fn present_error(parent: &gtk::Button, message: &str) {
    let alert = adw::AlertDialog::new(Some(&bookie_app_services::tr!("Snippet not saved")), Some(message));
    alert.add_response("close", &bookie_app_services::tr!("Close"));
    alert.present(Some(parent));
}

impl SqlEditor {
    pub(super) fn insert_snippet(&self, body: &str) {
        insert_expansion(&self.source_view.buffer(), &snippets::expand(body));
        self.source_view.grab_focus();
    }
}

pub(super) fn insert_expansion(buffer: &gtk::TextBuffer, expansion: &Expansion) {
    buffer.begin_user_action();
    buffer.delete_selection(true, true);
    let start = buffer.iter_at_mark(&buffer.get_insert()).offset();
    buffer.insert_at_cursor(&expansion.text);
    let before_cursor = expansion.text[..expansion.cursor].chars().count();
    let target = start + i32::try_from(before_cursor).unwrap_or(0);
    buffer.place_cursor(&buffer.iter_at_offset(target));
    buffer.end_user_action();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer_with(text: &str) -> gtk::TextBuffer {
        gtk::init().unwrap();
        let buffer = gtk::TextBuffer::new(None);
        buffer.set_text(text);
        buffer
    }

    fn text(buffer: &gtk::TextBuffer) -> String {
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), true).to_string()
    }

    fn cursor(buffer: &gtk::TextBuffer) -> i32 {
        buffer.iter_at_mark(&buffer.get_insert()).offset()
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn a_snippet_lands_at_the_cursor_and_leaves_the_cursor_at_its_marker() {
        let buffer = buffer_with("SELECT 1;\n");
        buffer.place_cursor(&buffer.end_iter());
        insert_expansion(&buffer, &snippets::expand("SELECT * FROM $0 LIMIT 10;"));
        assert_eq!(text(&buffer), "SELECT 1;\nSELECT * FROM  LIMIT 10;");
        assert_eq!(cursor(&buffer), "SELECT 1;\nSELECT * FROM ".chars().count() as i32);
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn a_snippet_replaces_the_selection_and_counts_characters_not_bytes() {
        let buffer = buffer_with("-- ünï\nold");
        let start = buffer.iter_at_offset(7);
        buffer.select_range(&start, &buffer.end_iter());
        insert_expansion(&buffer, &snippets::expand("é$0x"));
        assert_eq!(text(&buffer), "-- ünï\néx");
        assert_eq!(cursor(&buffer), 8);
    }
}
