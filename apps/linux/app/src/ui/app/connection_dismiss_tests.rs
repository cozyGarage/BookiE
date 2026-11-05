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
fn force_dismissing_disconnect_confirmation_preserves_the_dirty_editor() {
    use crate::services::{database_service, persistence_stores, preferences, workspace_state};
    adw::init().unwrap();
    gtk4::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk4::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("query.sql");
    std::fs::write(&path, "SELECT 1;").unwrap();
    let controller = App::builder()
        .launch(super::super::AppInit {
            registry: Arc::new(bookie_core::DriverRegistry::new()),
            persistence: persistence_stores::PersistenceStores {
                column_widths: None,
                filter_settings: None,
                csv_import_mappings: None,
                named_filter_presets: None,
            },
            workspace: workspace_state::WorkspaceStore::new(None, directory.path().join("drafts")),
            history: None,
            database: Arc::new(database_service::DatabaseService::new_isolated()),
            preferences: preferences::PreferencesStore::new(),
            mcp_bridge: None,
        })
        .detach();
    controller.widget().present();
    controller.emit(AppMsg::SqlFileOpened(
        bookie_core::text_file::read_text_file(&path, 1024).unwrap(),
    ));
    spin_until(|| controller.model().workspace_tabs.borrow().values().count() == 1);
    let id = controller.model().selected_workspace_tab_id().unwrap();
    let editor = match controller.model().workspace_tabs.borrow().get(&id).unwrap() {
        crate::ui::app::WorkspaceTab::Editor(slot) => source_view(slot.controller.widget().upcast_ref()).unwrap(),
        _ => panic!("expected editor"),
    };
    editor.buffer().set_text("SELECT 2;");
    controller.model().change_trackers.open_tab(id);
    controller.model().change_trackers.with_tab(id, |tracker| {
        tracker.track_insert(vec![bookie_core::Value::Int(1)]);
    });
    controller.emit(AppMsg::Disconnect);
    spin_until(|| controller.widget().visible_dialog().is_some());
    controller.widget().visible_dialog().unwrap().force_close();
    spin_until(|| controller.widget().visible_dialog().is_none());
    assert!(controller.model().workspace_tabs.borrow().contains_key(&id));
    assert!(
        controller
            .model()
            .change_trackers
            .with_tab_ref(id, |tracker| tracker.has_pending())
            .unwrap()
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "SELECT 1;");
    assert_eq!(
        editor
            .buffer()
            .text(&editor.buffer().start_iter(), &editor.buffer().end_iter(), true),
        "SELECT 2;"
    );
}
