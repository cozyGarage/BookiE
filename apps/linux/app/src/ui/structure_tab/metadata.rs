use super::StructureMode;
use relm4::adw::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MetadataAvailability {
    NotApplicable,
    Supported,
    Unsupported,
}

pub(super) fn metadata_availability(mode: StructureMode, supported: bool) -> MetadataAvailability {
    match mode {
        StructureMode::New => MetadataAvailability::NotApplicable,
        StructureMode::Edit if supported => MetadataAvailability::Supported,
        StructureMode::Edit => MetadataAvailability::Unsupported,
    }
}

pub(super) fn metadata_notice_row(title: &str, description: &str) -> relm4::adw::ActionRow {
    let row = relm4::adw::ActionRow::builder()
        .title(title)
        .subtitle(description)
        .build();
    row.add_prefix(&relm4::gtk::Image::from_icon_name("dialog-information-symbolic"));
    row.add_css_class("dim-label");
    row
}

#[cfg(test)]
mod tests {
    use super::{MetadataAvailability, metadata_availability};
    use crate::ui::structure_tab::StructureMode;
    use relm4::adw::prelude::*;
    use relm4::{Component, ComponentController};
    use tablepro_app_services::structure_metadata::StructureMetadata;

    use super::super::{StructureTab, StructureTabInit, StructureTabInput};

    #[test]
    fn unsupported_metadata_is_distinguished_from_supported_empty_metadata() {
        assert_eq!(
            metadata_availability(StructureMode::Edit, false),
            MetadataAvailability::Unsupported
        );
        assert_eq!(
            metadata_availability(StructureMode::Edit, true),
            MetadataAvailability::Supported
        );
    }

    #[test]
    fn new_table_metadata_is_not_misrepresented_as_a_read_result() {
        assert_eq!(
            metadata_availability(StructureMode::New, false),
            MetadataAvailability::NotApplicable
        );
    }

    #[test]
    fn registered_driver_metadata_profiles_reach_the_structure_consumer() {
        let registry = crate::build_registry();

        let postgres = super::super::driver_capabilities_for(&registry, "postgres");
        assert!(postgres.supports_index_metadata);
        assert!(postgres.supports_foreign_key_metadata);
        let redis = super::super::driver_capabilities_for(&registry, "redis");
        assert!(!redis.supports_index_metadata);
        assert!(!redis.supports_foreign_key_metadata);
        assert_eq!(
            super::super::driver_capabilities_for(&registry, "missing"),
            tablepro_core::DriverCapabilities::default()
        );
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn metadata_notice_explains_that_reads_are_unavailable() {
        relm4::gtk::init().unwrap();

        let mut controller = StructureTab::builder()
            .launch(StructureTabInit {
                tab_id: uuid::Uuid::new_v4(),
                schema: Some("public".into()),
                table: "items".into(),
                mode: StructureMode::Edit,
                driver_id: "postgres".into(),
                column_comment_capability: Default::default(),
                defer_initial_fetch: true,
            })
            .detach();

        for supported in [false, true] {
            controller.emit(StructureTabInput::StructureLoaded {
                columns: Vec::new(),
                metadata: StructureMetadata {
                    indexes_supported: supported,
                    foreign_keys_supported: supported,
                    ..StructureMetadata::default()
                },
            });
            wait_for_metadata_rows(&controller, if supported { 1 } else { 2 });
            let model = controller.model();
            for container in [&model.indexes_box, &model.fks_box] {
                let rows = metadata_rows(container);
                let add = rows.last().unwrap();
                assert!(add.is_sensitive());
                assert!(add.is_activatable());
                if !supported {
                    assert!(!rows[0].is_activatable());
                    assert!(rows[0].subtitle().is_some_and(|text| !text.is_empty()));
                }
            }
        }
        controller.detach_runtime();
    }

    fn metadata_rows(container: &relm4::gtk::Box) -> Vec<relm4::adw::ActionRow> {
        let list = container
            .first_child()
            .unwrap()
            .downcast::<relm4::gtk::ListBox>()
            .unwrap();
        (0..)
            .map_while(|index| list.row_at_index(index))
            .map(|row| row.downcast::<relm4::adw::ActionRow>().unwrap())
            .collect()
    }

    fn wait_for_metadata_rows(controller: &relm4::Controller<StructureTab>, count: usize) {
        let context = relm4::gtk::glib::MainContext::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            context.iteration(false);
            let model = controller.model();
            if model.inner_stack.visible_child_name().as_deref() == Some("editor")
                && metadata_rows(&model.indexes_box).len() == count
                && metadata_rows(&model.fks_box).len() == count
            {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "metadata rows were not rendered");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}
