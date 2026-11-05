use super::*;

fn button(widget: &relm4::gtk::Widget, label: &str) -> Option<relm4::gtk::Button> {
    use relm4::gtk::prelude::*;
    if let Some(button) = widget.downcast_ref::<relm4::gtk::Button>()
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
#[ignore = "requires isolated GTK display"]
fn dismissing_bulk_delete_preserves_selected_drafts() {
    use relm4::adw::prelude::*;
    relm4::adw::init().unwrap();
    let app = relm4::adw::Application::builder()
        .application_id("com.bookie.linux.bulk-delete-test")
        .build();
    app.register(None::<&relm4::gtk::gio::Cancellable>).unwrap();
    let (mut controller, observed) = launch(Uuid::new_v4());
    let window = relm4::adw::ApplicationWindow::new(&app);
    window.set_content(Some(controller.widget()));
    window.present();
    load_two_rows(&controller, &observed);
    for index in 0..10 {
        add_draft_row(
            &controller,
            vec![
                Value::Int(index),
                Value::Text("draft".into()),
                Value::Text("body".into()),
            ],
        );
    }
    pump(&observed);
    let selection = controller.model().current_selection.clone().unwrap();
    for index in 0..10 {
        selection.select_item(index, false);
    }
    assert!(
        (0..10).all(|index| selection.is_selected(index)),
        "selection must contain all ten drafts before deletion"
    );
    controller.sender().send(BrowseTabInput::DeleteSelectedRow).unwrap();
    pump(&observed);
    window.visible_dialog().unwrap().force_close();
    pump(&observed);
    assert_eq!(pending(&controller), 10);
    assert_eq!(selection.n_items(), 12);
    assert!((0..10).all(|index| selection.is_selected(index)));
    window.close();
    controller.detach_runtime();
}

#[test]
#[ignore = "requires isolated GTK display"]
fn confirming_bulk_delete_removes_selected_drafts_once() {
    use relm4::adw::prelude::*;
    relm4::adw::init().unwrap();
    let app = relm4::adw::Application::builder()
        .application_id("com.bookie.linux.bulk-delete-confirm-test")
        .build();
    app.register(None::<&relm4::gtk::gio::Cancellable>).unwrap();
    let (mut controller, observed) = launch(Uuid::new_v4());
    let window = relm4::adw::ApplicationWindow::new(&app);
    window.set_content(Some(controller.widget()));
    window.present();
    load_two_rows(&controller, &observed);
    for index in 0..10 {
        add_draft_row(
            &controller,
            vec![
                Value::Int(index),
                Value::Text("draft".into()),
                Value::Text("body".into()),
            ],
        );
    }
    pump(&observed);
    let selection = controller.model().current_selection.clone().unwrap();
    for index in 0..10 {
        selection.select_item(index, false);
    }
    controller.sender().send(BrowseTabInput::DeleteSelectedRow).unwrap();
    pump(&observed);
    let dialog = window.visible_dialog().unwrap();
    button(&dialog.upcast::<relm4::gtk::Widget>(), "Delete")
        .expect("delete confirmation button is visible")
        .emit_clicked();
    pump(&observed);
    assert_eq!(pending(&controller), 0);
    assert_eq!(selection.n_items(), 2);
    assert!(window.visible_dialog().is_none());
    window.close();
    controller.detach_runtime();
}
