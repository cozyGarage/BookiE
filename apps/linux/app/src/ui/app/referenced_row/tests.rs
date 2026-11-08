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

#[test]
#[ignore = "requires an isolated GTK display"]
fn force_dismissing_referenced_row_choice_keeps_the_window_usable() {
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
    controller.emit(super::super::AppMsg::ChooseReferencedRow(vec![
        crate::ui::browse_tab::reference::ReferencedRowTarget {
            foreign_key: "orders_customer_id_fkey".into(),
            schema: Some("public".into()),
            table: "customers".into(),
            filter: bookie_core::FilterSet::default(),
        },
    ]));
    spin_until(|| controller.widget().visible_dialog().is_some());
    controller.widget().visible_dialog().unwrap().force_close();
    spin_until(|| controller.widget().visible_dialog().is_none());
    assert!(controller.widget().is_visible());
}
