use relm4::{Component, ComponentController};
use tablepro_core::EnumInputStyle;
use uuid::Uuid;

use super::tests::spin_for;
use super::{BrowseTab, BrowseTabInit, BrowseTabInput};

#[test]
#[ignore = "requires isolated GTK display"]
fn a_filter_is_ignored_when_the_driver_cannot_filter_a_browse() {
    relm4::gtk::init().unwrap();
    let controller = BrowseTab::builder()
        .launch(BrowseTabInit {
            tab_id: Uuid::new_v4(),
            schema: None,
            table: "db0".into(),
            driver_id: "redis".into(),
            enum_input_style: EnumInputStyle::RawValue,
            browse_filters: false,
            connection_id: None,
            read_only: true,
            page_size: 100,
            initial_offset: 0,
            initial_sort: None,
            persistence: crate::services::persistence_stores::PersistenceStores {
                column_widths: None,
                filter_settings: None,
                csv_import_mappings: None,
                named_filter_presets: None,
            },
            database: std::sync::Arc::new(crate::services::database_service::DatabaseService::new_isolated()),
        })
        .detach();
    let rule = tablepro_core::equality_rule("Key", &tablepro_core::Value::Text("a".into())).unwrap();
    let context = relm4::gtk::glib::MainContext::default();

    controller
        .sender()
        .send(BrowseTabInput::FilterApplied(
            tablepro_core::FilterSet::default().narrowed_to(rule),
        ))
        .unwrap();
    spin_for(&context, std::time::Duration::from_millis(200));

    assert_eq!(controller.model().current_filter.len(), 0);
}

#[test]
#[ignore = "requires isolated GTK display"]
fn a_saved_filter_is_not_restored_when_the_driver_cannot_filter_a_browse() {
    relm4::gtk::init().unwrap();
    let connection_id = Uuid::new_v4();
    let store = crate::services::filter_settings::FilterSettingsStore::open().unwrap();
    let rule = tablepro_core::equality_rule("Key", &tablepro_core::Value::Text("a".into())).unwrap();
    store
        .save(
            connection_id,
            None,
            "db0",
            tablepro_core::FilterSet::default().narrowed_to(rule),
        )
        .unwrap();
    assert_eq!(store.load(connection_id, None, "db0").len(), 1);
    let controller = BrowseTab::builder()
        .launch(BrowseTabInit {
            tab_id: Uuid::new_v4(),
            schema: None,
            table: "db0".into(),
            driver_id: "redis".into(),
            enum_input_style: EnumInputStyle::RawValue,
            browse_filters: false,
            connection_id: Some(connection_id),
            read_only: true,
            page_size: 100,
            initial_offset: 0,
            initial_sort: None,
            persistence: crate::services::persistence_stores::PersistenceStores {
                column_widths: None,
                filter_settings: Some(store),
                csv_import_mappings: None,
                named_filter_presets: None,
            },
            database: std::sync::Arc::new(crate::services::database_service::DatabaseService::new_isolated()),
        })
        .detach();

    assert_eq!(controller.model().current_filter.len(), 0);
}
