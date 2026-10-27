use super::*;
use crate::ui::structure_tab::{StructureMode, StructureTabInit};
use relm4::{Component, ComponentController};

#[test]
#[ignore = "requires an isolated GTK display"]
fn dismissing_index_setup_releases_its_dialog() {
    assert_form_released("Add Index…", false);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn dismissing_foreign_key_setup_releases_its_dialog() {
    assert_form_released("Add Foreign Key…", false);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn submitting_index_setup_updates_preview_and_releases_its_dialog() {
    assert_form_released("Add Index…", true);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn submitting_foreign_key_setup_updates_preview_and_releases_its_dialog() {
    assert_form_released("Add Foreign Key…", true);
}

fn assert_form_released(title: &str, submit: bool) {
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
    if submit {
        controller.emit(StructureTabInput::AddColumn);
        while context.iteration(false) {}
    }
    find_action(controller.widget().upcast_ref(), title)
        .unwrap()
        .emit_by_name::<()>("activated", &[]);
    let dialog = window.visible_dialog().unwrap();
    let retired = dialog.downgrade();
    if submit {
        submit_form(&dialog, title == "Add Foreign Key…");
        while context.iteration(false) {}
        let preview = descendants(controller.widget().upcast_ref())
            .into_iter()
            .find_map(|widget| widget.downcast::<gtk::TextView>().ok())
            .unwrap()
            .buffer();
        let (start, end) = preview.bounds();
        let sql = preview.text(&start, &end, true);
        assert!(
            sql.contains("form_test"),
            "submitted metadata is absent from SQL preview"
        );
        if title == "Add Foreign Key…" {
            assert!(sql.contains("parent_items"));
        }
    } else {
        dialog.close();
    }
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
    descendants(widget)
        .into_iter()
        .filter_map(|widget| widget.downcast::<adw::ActionRow>().ok())
        .find(|row| row.title() == title)
}

fn submit_form(dialog: &adw::Dialog, foreign_key: bool) {
    let widgets = descendants(dialog.upcast_ref());
    for widget in &widgets {
        if let Some(row) = widget.downcast_ref::<adw::EntryRow>() {
            match row.title().as_str() {
                "Name" => row.set_text("form_test"),
                "Table" if foreign_key => row.set_text("parent_items"),
                "Columns" if foreign_key => row.set_text("id"),
                _ => {}
            }
        }
    }
    widgets
        .iter()
        .find_map(|widget| widget.downcast_ref::<gtk::CheckButton>())
        .unwrap()
        .set_active(true);
    widgets
        .iter()
        .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
        .find(|button| button.label().as_deref() == Some("Add"))
        .unwrap()
        .emit_clicked();
}

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut widgets = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        widgets.extend(descendants(&current));
    }
    widgets
}
