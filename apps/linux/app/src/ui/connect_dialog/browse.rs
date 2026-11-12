use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};

use bookie_core::DatabaseDriver;

use super::{ConnectDialog, ConnectDialogInput};

fn picker_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    button
}

fn file_filters(name: &str, patterns: &[&str]) -> gtk::gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(name));
    for pattern in patterns {
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
    attach_path_picker(
        row,
        &bookie_app_services::tr!("Browse for certificate authority file"),
        &bookie_app_services::tr!("Select certificate authority"),
        &bookie_app_services::tr!("Certificates"),
        &["*.pem", "*.crt", "*.cer"],
    );
}

pub(super) fn attach_client_certificate_picker(row: &adw::EntryRow) {
    attach_path_picker(
        row,
        &bookie_app_services::tr!("Browse for client certificate file"),
        &bookie_app_services::tr!("Select client certificate"),
        &bookie_app_services::tr!("Certificates"),
        &["*.pem", "*.crt", "*.cer"],
    );
}

pub(super) fn attach_client_key_picker(row: &adw::EntryRow) {
    attach_path_picker(
        row,
        &bookie_app_services::tr!("Browse for client private key file"),
        &bookie_app_services::tr!("Select client private key"),
        &bookie_app_services::tr!("Private keys"),
        &["*.pem", "*.key"],
    );
}

fn attach_path_picker(row: &adw::EntryRow, tooltip: &str, title: &str, filter_name: &str, patterns: &[&str]) {
    let button = picker_button("document-open-symbolic", tooltip);
    let title = title.to_string();
    let filter_name = filter_name.to_string();
    let patterns: Vec<String> = patterns.iter().map(|pattern| (*pattern).to_string()).collect();
    let entry = row.clone();
    button.connect_clicked(move |button| {
        let pattern_refs: Vec<&str> = patterns.iter().map(String::as_str).collect();
        let dialog = gtk::FileDialog::builder()
            .title(&title)
            .modal(true)
            .filters(&file_filters(&filter_name, &pattern_refs))
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

pub(super) fn request_database_file(
    driver: Arc<dyn DatabaseDriver>,
    path: PathBuf,
    sender: &ComponentSender<ConnectDialog>,
) {
    let input = sender.input_sender().clone();
    gtk::glib::spawn_future_local(async move {
        let created = gtk::gio::spawn_blocking(move || {
            let outcome = bookie_app_services::database_file::create_database_file(driver.as_ref(), &path);
            (path, outcome)
        })
        .await;
        let Ok((path, outcome)) = created else {
            tracing::warn!("creating the database file did not finish");
            return;
        };
        input.emit(ConnectDialogInput::DatabaseFileCreated { path, outcome });
    });
}

pub(super) fn attach_database_file_picker(row: &adw::EntryRow, on_create: impl Fn(PathBuf) + 'static) -> gtk::Box {
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let open = picker_button(
        "document-open-symbolic",
        &bookie_app_services::tr!("Choose an existing database file"),
    );
    let entry = row.clone();
    open.connect_clicked(move |button| {
        let dialog = gtk::FileDialog::builder()
            .title(bookie_app_services::tr!("Choose database file"))
            .modal(true)
            .build();
        let entry = entry.clone();
        dialog.open(
            parent_window(button).as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| set_entry_from_file(&entry, result),
        );
    });
    let create = picker_button(
        "document-new-symbolic",
        &bookie_app_services::tr!("Create a new database file"),
    );
    let on_create = Rc::new(on_create);
    create.connect_clicked(move |button| {
        let dialog = gtk::FileDialog::builder()
            .title(bookie_app_services::tr!("Create database file"))
            .modal(true)
            .build();
        let on_create = on_create.clone();
        dialog.save(
            parent_window(button).as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| {
                if let Some(path) = result.ok().and_then(|file| file.path()) {
                    on_create(path);
                }
            },
        );
    });
    buttons.append(&open);
    buttons.append(&create);
    row.add_suffix(&buttons);
    buttons
}
