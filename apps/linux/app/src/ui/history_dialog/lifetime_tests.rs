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
