use super::*;

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
