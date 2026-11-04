use super::*;
use relm4::{Component, ComponentController};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn spin_until(mut condition: impl FnMut() -> bool) {
    let context = gtk4::glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() && Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(condition());
}

fn launch(directory: &std::path::Path) -> relm4::Controller<App> {
    use crate::services::{database_service, persistence_stores, preferences, workspace_state};
    App::builder()
        .launch(super::super::AppInit {
            registry: Arc::new(bookie_core::DriverRegistry::new()),
            persistence: persistence_stores::PersistenceStores {
                column_widths: None,
                filter_settings: None,
                csv_import_mappings: None,
                named_filter_presets: None,
            },
            workspace: workspace_state::WorkspaceStore::new(None, directory.join("drafts")),
            history: None,
            database: Arc::new(database_service::DatabaseService::new_isolated()),
            preferences: preferences::PreferencesStore::new(),
            mcp_bridge: None,
        })
        .detach()
}

fn source_view(widget: &gtk4::Widget) -> Option<sourceview5::View> {
    if let Some(view) = widget.downcast_ref::<sourceview5::View>() {
        return Some(view.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(view) = source_view(&current) {
            return Some(view);
        }
    }
    None
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn forced_dirty_tab_confirmation_dismissal_preserves_the_file_and_tab() {
    adw::init().unwrap();
    gtk4::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk4::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("query.sql");
    std::fs::write(&path, "SELECT 1;").unwrap();
    let controller = launch(directory.path());
    controller.widget().present();
    controller.emit(AppMsg::SqlFileOpened(
        bookie_core::text_file::read_text_file(&path, 1024).unwrap(),
    ));
    spin_until(|| controller.model().workspace_tabs.borrow().values().count() == 1);
    let id = controller.model().selected_workspace_tab_id().unwrap();
    let editor = match controller.model().workspace_tabs.borrow().get(&id).unwrap() {
        WorkspaceTab::Editor(slot) => source_view(slot.controller.widget().upcast_ref()).unwrap(),
        _ => panic!("expected editor"),
    };
    editor.buffer().set_text("SELECT 2;");
    spin_until(|| controller.model().pending_workspace_tab_changes(id).file);
    controller.emit(AppMsg::CloseActiveWorkspaceTab);
    spin_until(|| controller.widget().visible_dialog().is_some());
    controller.widget().visible_dialog().unwrap().force_close();
    spin_until(|| controller.widget().visible_dialog().is_none());
    assert!(controller.model().workspace_tabs.borrow().contains_key(&id));
    assert!(controller.model().pending_workspace_tab_changes(id).file);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "SELECT 1;");
    assert_eq!(
        editor
            .buffer()
            .text(&editor.buffer().start_iter(), &editor.buffer().end_iter(), true),
        "SELECT 2;"
    );
    respond(&controller, "Cancel");
    assert!(controller.model().workspace_tabs.borrow().contains_key(&id));
    assert!(controller.model().pending_workspace_tab_changes(id).file);
    respond(&controller, "Discard");
    spin_until(|| !controller.model().workspace_tabs.borrow().contains_key(&id));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "SELECT 1;");
}

fn respond(controller: &relm4::Controller<App>, label: &str) {
    controller.emit(AppMsg::CloseActiveWorkspaceTab);
    spin_until(|| controller.widget().visible_dialog().is_some());
    let dialog = controller.widget().visible_dialog().unwrap();
    response_button(dialog.upcast_ref(), label).unwrap().emit_clicked();
    spin_until(|| controller.widget().visible_dialog().is_none());
}

fn response_button(widget: &gtk4::Widget, label: &str) -> Option<gtk4::Button> {
    if let Some(button) = widget.downcast_ref::<gtk4::Button>()
        && button.label().as_deref() == Some(label)
    {
        return Some(button.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(button) = response_button(&current, label) {
            return Some(button);
        }
    }
    None
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn force_dismissing_connection_deletion_does_not_abort() {
    adw::init().unwrap();
    gtk4::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk4::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let controller = launch(directory.path());
    controller.widget().present();
    controller.emit(AppMsg::DeleteConnection(Uuid::new_v4()));
    spin_until(|| controller.widget().visible_dialog().is_some());
    controller.widget().visible_dialog().unwrap().force_close();
    spin_until(|| controller.widget().visible_dialog().is_none());
}
