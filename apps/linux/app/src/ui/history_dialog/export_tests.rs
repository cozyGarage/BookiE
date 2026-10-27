use super::*;
use std::sync::Arc;

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_failed_history_export_shows_a_dismissible_error() {
    adw::init().unwrap();
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
    assert_eq!(alert.heading().as_deref(), Some("Could not export query history"));
    assert!(alert.has_response("close"));
    alert.force_close();
    window.close();
    drop(controller);
    drop(entered);
}
