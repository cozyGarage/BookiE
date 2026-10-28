use super::*;
use std::sync::Arc;

#[test]
#[ignore = "requires an isolated GTK display"]
fn refreshing_history_releases_removed_rows() {
    assert_rows_released(false);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn closing_history_releases_its_rows_and_popovers() {
    assert_rows_released(true);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn history_filter_popover_stays_inside_narrow_windows() {
    adw::init().unwrap();
    let app = adw::Application::builder()
        .application_id("com.tablepro.linux.history-popover-test")
        .build();
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
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
    drop(entered);
    let root = controller.model().root.clone();
    let filter_button = find_filter_button(root.upcast_ref()).unwrap();
    let filter_popover = filter_button.popover().unwrap().downcast::<gtk::Popover>().unwrap();
    let window = adw::ApplicationWindow::builder()
        .application(&app)
        .default_width(520)
        .default_height(600)
        .content(&root)
        .build();
    window.present();
    wait_until(|| window.is_mapped());
    filter_button.popup();
    wait_until(|| filter_popover.is_visible() && filter_popover.width() > 100);
    let bounds = filter_popover.compute_bounds(&window).unwrap();
    assert!(
        bounds.x() >= 0.0 && bounds.x() + bounds.width() <= window.width() as f32,
        "popover x={} width={} window width={}",
        bounds.x(),
        bounds.width(),
        window.width()
    );
    window.close();
    drop(controller);
}

fn assert_rows_released(close: bool) {
    adw::init().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let history = runtime
        .block_on(HistoryStore::open(directory.path().join("history.db")))
        .unwrap();
    runtime
        .block_on(history.record(tablepro_storage::query_history::NewEntry {
            query: "SELECT 1728".into(),
            driver_id: "sqlite".into(),
            connection_id: uuid::Uuid::new_v4(),
            connection_name: "synthetic".into(),
            executed_at: SystemTime::now(),
            duration_ms: Some(1),
            rows_affected: Some(1),
            outcome: tablepro_storage::query_history::Outcome::Success,
            source: Source::Editor,
        }))
        .unwrap();
    let controller = HistoryDialog::builder()
        .launch(HistoryDialogInit {
            history: history.clone(),
            database: Arc::new(crate::services::database_service::DatabaseService::new_isolated()),
        })
        .detach();
    drop(entered);
    wait_until(|| controller.model().listbox.first_child().is_some());
    let row = controller.model().listbox.first_child().unwrap().downgrade();
    if close {
        let popover = controller.model().row_popovers[0].clone();
        drop(controller);
        wait_until(|| popover.parent().is_none());
        drop(popover);
        wait_until(|| row.upgrade().is_none());
    } else {
        runtime.block_on(history.clear_all()).unwrap();
        controller.emit(HistoryDialogInput::Refresh);
        wait_until(|| controller.model().listbox.first_child().is_none());
        assert!(row.upgrade().is_none(), "removed history row is retained");
        drop(controller);
    }
}

fn wait_until(predicate: impl Fn() -> bool) {
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        while context.iteration(false) {}
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(predicate(), "history update did not settle");
}

fn find_filter_button(widget: &gtk::Widget) -> Option<gtk::MenuButton> {
    if let Ok(button) = widget.clone().downcast::<gtk::MenuButton>()
        && button.tooltip_text().as_deref() == Some("Filter history")
    {
        return Some(button);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(button) = find_filter_button(&current) {
            return Some(button);
        }
        child = current.next_sibling();
    }
    None
}
