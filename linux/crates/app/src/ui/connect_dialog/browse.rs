use relm4::adw::prelude::*;
use relm4::{adw, gtk};

fn picker_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    button
}

fn certificate_filters() -> gtk::gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&crate::tr!("Certificates")));
    for pattern in ["*.pem", "*.crt", "*.cer"] {
        filter.add_pattern(pattern);
    }
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}

fn set_entry_from_file(entry: &adw::EntryRow, result: Result<gtk::gio::File, gtk::glib::Error>) {
    if let Ok(file) = result
        && let Some(path) = file.path()
    {
        entry.set_text(&path.to_string_lossy());
    }
}

fn parent_window(button: &gtk::Button) -> Option<gtk::Window> {
    button.root().and_then(|root| root.downcast::<gtk::Window>().ok())
}

pub(super) fn attach_certificate_picker(row: &adw::EntryRow) {
    let button = picker_button(
        "document-open-symbolic",
        &crate::tr!("Browse for certificate authority file"),
    );
    let entry = row.clone();
    button.connect_clicked(move |button| {
        let dialog = gtk::FileDialog::builder()
            .title(crate::tr!("Select certificate authority"))
            .modal(true)
            .filters(&certificate_filters())
            .build();
        let entry = entry.clone();
        dialog.open(
            parent_window(button).as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| set_entry_from_file(&entry, result),
        );
    });
    row.add_suffix(&button);
}

pub(super) fn attach_database_file_picker(row: &adw::EntryRow) -> gtk::Box {
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let open = picker_button(
        "document-open-symbolic",
        &crate::tr!("Choose an existing database file"),
    );
    let entry = row.clone();
    open.connect_clicked(move |button| {
        let dialog = gtk::FileDialog::builder()
            .title(crate::tr!("Choose database file"))
            .modal(true)
            .build();
        let entry = entry.clone();
        dialog.open(
            parent_window(button).as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| set_entry_from_file(&entry, result),
        );
    });
    let create = picker_button("document-new-symbolic", &crate::tr!("Create a new database file"));
    let entry = row.clone();
    create.connect_clicked(move |button| {
        let dialog = gtk::FileDialog::builder()
            .title(crate::tr!("Create database file"))
            .modal(true)
            .build();
        let entry = entry.clone();
        dialog.save(
            parent_window(button).as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| set_entry_from_file(&entry, result),
        );
    });
    buttons.append(&open);
    buttons.append(&create);
    row.add_suffix(&buttons);
    buttons
}
