use super::*;
use relm4::{Component, ComponentController, gtk};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn spin_until(mut condition: impl FnMut() -> bool) {
    let context = gtk::glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() && Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(condition());
}

fn button(widget: &gtk::Widget, label: &str) -> Option<gtk::Button> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>()
        && button.label().as_deref() == Some(label)
    {
        return Some(button.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(button) = button(&current, label) {
            return Some(button);
        }
    }
    None
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn forced_connection_failure_dismissal_keeps_the_window_usable() {
    use crate::services::{database_service, persistence_stores, preferences, workspace_state};
    adw::init().unwrap();
    gtk::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let controller = App::builder()
        .launch(super::super::AppInit {
            registry: Arc::new(bookie_core::DriverRegistry::new()),
            persistence: persistence_stores::PersistenceStores {
                column_widths: None,
                filter_settings: None,
                highlight_settings: None,
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
    let saved = bookie_storage::SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: "Test connection".into(),
        driver_id: "missing-test-driver".into(),
        host: "localhost".into(),
        port: 5432,
        socket_dir: None,
        socket_path: None,
        database: "test".into(),
        username: "test".into(),
        use_tls: false,
        tls_mode: Some(bookie_core::TlsMode::Disabled),
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: false,
        auth_mode: bookie_core::AuthMode::Password,
        environment: bookie_core::Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        attached_databases: Vec::new(),
        tls_strict_encryption: false,
    };
    controller.emit(AppMsg::OpenSaved(saved.clone()));
    spin_until(|| controller.widget().visible_dialog().is_some());
    let dialog = controller.widget().visible_dialog().unwrap();
    let alert = dialog.clone().downcast::<adw::AlertDialog>().unwrap();
    assert_eq!(alert.heading().as_deref(), Some("Connection failed"));
    dialog.force_close();
    spin_until(|| controller.widget().visible_dialog().is_none());
    assert!(controller.widget().is_visible());
    controller.emit(AppMsg::OpenSaved(saved));
    spin_until(|| controller.widget().visible_dialog().is_some());
    let dialog = controller.widget().visible_dialog().unwrap();
    button(dialog.upcast_ref(), "Close").unwrap().emit_clicked();
    spin_until(|| controller.widget().visible_dialog().is_none());
    assert!(controller.widget().is_visible());
}
