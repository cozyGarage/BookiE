use super::*;
use relm4::{Component, ComponentController};

#[test]
#[ignore = "requires an isolated GTK display"]
fn closing_a_structure_tab_releases_its_generated_views() {
    adw::init().unwrap();
    let controller = StructureTab::builder()
        .launch(StructureTabInit {
            tab_id: Uuid::new_v4(),
            schema: None,
            table: "items".into(),
            mode: StructureMode::New,
            driver_id: "sqlite".into(),
            column_comment_capability: Default::default(),
            defer_initial_fetch: true,
        })
        .detach();
    let context = gtk::glib::MainContext::default();
    while context.iteration(false) {}
    let retired = {
        let model = controller.model();
        (
            model.indexes_box.downgrade(),
            model.fks_box.downgrade(),
            model.sql_buffer.downgrade(),
        )
    };
    drop(controller);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(retired.0.upgrade().is_none(), "index view remains after tab close");
    assert!(
        retired.1.upgrade().is_none(),
        "foreign key view remains after tab close"
    );
    assert!(retired.2.upgrade().is_none(), "SQL preview remains after tab close");
}
