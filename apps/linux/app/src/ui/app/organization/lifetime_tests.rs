use super::*;
use relm4::{Component, ComponentController};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires an isolated GTK display"]
fn force_dismissing_group_and_tags_keeps_the_window_usable() {
    let (controller, _directory) = app_controller();
    controller.emit(AppMsg::OrganizeConnection(saved_connection()));
    dismiss_visible_dialog(&controller);
    assert!(controller.widget().is_visible());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn force_dismissing_connection_url_import_keeps_the_window_usable() {
    let (controller, _directory) = app_controller();
    controller.emit(AppMsg::ImportConnectionUrl);
    dismiss_visible_dialog(&controller);
    assert!(controller.widget().is_visible());
}

fn app_controller() -> (relm4::Controller<App>, tempfile::TempDir) {
    use crate::services::{database_service, persistence_stores, preferences, workspace_state};

    adw::init().unwrap();
    gtk::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let controller = App::builder()
        .launch(crate::ui::app::AppInit {
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
    (controller, directory)
}

fn dismiss_visible_dialog(controller: &relm4::Controller<App>) {
    spin_until(|| controller.widget().visible_dialog().is_some());
    let dialog = controller.widget().visible_dialog().unwrap();
    let closed = std::rc::Rc::new(std::cell::Cell::new(false));
    let closed_for_signal = closed.clone();
    dialog.connect_closed(move |_| closed_for_signal.set(true));
    dialog.force_close();
    spin_until(|| closed.get());
    spin_until(|| controller.widget().visible_dialog().is_none());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn dropping_the_colour_chooser_releases_its_popover() {
    adw::init().unwrap();
    let (row, selected) = color_chooser(0);
    let popup = find_popover(row.upcast_ref()).unwrap();
    let choices = popup.child().unwrap();
    let blue = choices
        .first_child()
        .unwrap()
        .next_sibling()
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    blue.emit_clicked();
    assert_eq!(selected.get(), 1);
    drop(blue);
    drop(choices);
    drop(popup);
    let popover = find_popover(row.upcast_ref()).unwrap().downgrade();
    drop(row);
    let context = gtk::glib::MainContext::default();
    while context.iteration(false) {}
    assert!(
        popover.upgrade().is_none(),
        "colour popover remains after its chooser is dropped"
    );
}

fn find_popover(widget: &gtk::Widget) -> Option<gtk::Popover> {
    if let Some(popover) = widget.downcast_ref::<gtk::Popover>() {
        return Some(popover.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(popover) = find_popover(&current) {
            return Some(popover);
        }
    }
    None
}

fn spin_until(mut condition: impl FnMut() -> bool) {
    let context = gtk::glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() && Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(condition());
}

fn saved_connection() -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: "Test connection".into(),
        driver_id: "postgres".into(),
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
    }
}
