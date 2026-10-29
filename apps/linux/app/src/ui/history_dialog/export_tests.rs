use super::*;
use std::sync::Arc;

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_failed_history_export_shows_a_dismissible_error() {
    adw::init().unwrap();
    gtk::Settings::default().unwrap().set_gtk_enable_animations(false);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let history = runtime
        .block_on(HistoryStore::open(directory.path().join("history.db")))
        .unwrap();
    let controller = HistoryDialog::builder()
        .launch(HistoryDialogInit {
            history,
            database: Arc::new(crate::services::database_service::DatabaseService::new_isolated()),
        })
        .detach();
    let application = adw::Application::builder()
        .application_id("com.tablepro.linux.history-export-test")
        .build();
    application.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    window.set_content(Some(controller.widget()));
    window.present();
    let context = glib::MainContext::default();
    while context.iteration(false) {}
    controller.model().export_failed("synthetic export failure");
    while context.iteration(false) {}
    let dialog = window.visible_dialog().expect("export failure must be visible");
    let alert = dialog.downcast::<adw::AlertDialog>().unwrap();
    assert!(alert.heading().is_some_and(|heading| !heading.is_empty()));
    assert!(!alert.body().is_empty());
    assert!(!alert.body().contains("synthetic export failure"));
    assert!(alert.has_response("close"));
    let closed = std::rc::Rc::new(std::cell::Cell::new(false));
    let observed = closed.clone();
    alert.connect_response(Some("close"), move |_, _| observed.set(true));
    assert!(alert.close());
    while context.iteration(false) {}
    assert!(closed.get());
    assert!(window.visible_dialog().is_none());
    window.close();
    drop(controller);
    drop(entered);
}
