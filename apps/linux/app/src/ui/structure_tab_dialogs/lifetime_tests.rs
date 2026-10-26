use super::*;
use crate::ui::structure_tab::{StructureMode, StructureTabInit};
use relm4::{Component, ComponentController};

#[test]
#[ignore = "requires an isolated GTK display"]
fn dismissing_index_setup_releases_its_dialog() {
    assert_form_released("Add Index…");
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn dismissing_foreign_key_setup_releases_its_dialog() {
    assert_form_released("Add Foreign Key…");
}

fn assert_form_released(title: &str) {
    adw::init().unwrap();
    let application = adw::Application::builder()
        .application_id("com.tablepro.linux.structure-lifetime-test")
        .build();
    application.register(None::<&gtk::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    let controller = StructureTab::builder()
        .launch(StructureTabInit {
            tab_id: uuid::Uuid::new_v4(),
            schema: None,
            table: "items".into(),
            mode: StructureMode::New,
            driver_id: "sqlite".into(),
            column_comment_capability: Default::default(),
            defer_initial_fetch: true,
        })
        .detach();
    window.set_content(Some(controller.widget()));
    window.present();
    let context = gtk::glib::MainContext::default();
    while context.iteration(false) {}
    find_action(controller.widget().upcast_ref(), title)
        .unwrap()
        .emit_by_name::<()>("activated", &[]);
    let dialog = window.visible_dialog().unwrap();
    let retired = dialog.downgrade();
    dialog.close();
    drop(dialog);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while retired.upgrade().is_some() && std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(retired.upgrade().is_none(), "closed structure form is still retained");
    window.close();
}

fn find_action(widget: &gtk::Widget, title: &str) -> Option<adw::ActionRow> {
    if let Some(row) = widget.downcast_ref::<adw::ActionRow>()
        && row.title() == title
    {
        return Some(row.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(row) = find_action(&current, title) {
            return Some(row);
        }
    }
    None
}
