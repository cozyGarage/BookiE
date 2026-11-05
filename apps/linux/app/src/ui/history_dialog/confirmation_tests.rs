use super::*;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy)]
enum Target {
    Selected,
    All,
    Preferences,
}

struct Fixture {
    runtime: tokio::runtime::Runtime,
    _directory: tempfile::TempDir,
    history: HistoryStore,
    controller: Controller<HistoryDialog>,
    window: adw::ApplicationWindow,
}

impl Fixture {
    fn new() -> Self {
        adw::init().unwrap();
        gtk::Settings::default().unwrap().set_gtk_enable_animations(false);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let entered = runtime.enter();
        let directory = tempfile::tempdir().unwrap();
        let history = runtime
            .block_on(HistoryStore::open(directory.path().join("history.db")))
            .unwrap();
        for query in ["SELECT 1", "SELECT 2"] {
            let id = runtime
                .block_on(history.record(bookie_storage::query_history::NewEntry {
                    query: query.into(),
                    driver_id: "sqlite".into(),
                    connection_id: uuid::Uuid::new_v4(),
                    connection_name: "synthetic".into(),
                    executed_at: SystemTime::now(),
                    duration_ms: Some(1),
                    rows_affected: Some(1),
                    outcome: bookie_storage::query_history::Outcome::Success,
                    source: Source::Editor,
                }))
                .unwrap()
                .unwrap();
            runtime.block_on(history.set_pinned(id, true)).unwrap();
        }
        let controller = HistoryDialog::builder()
            .launch(HistoryDialogInit {
                history: history.clone(),
                database: Arc::new(crate::services::database_service::DatabaseService::new_isolated()),
            })
            .detach();
        drop(entered);
        let app = adw::Application::builder()
            .application_id("com.bookie.linux.history-confirmation-test")
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        window.set_content(Some(controller.widget()));
        window.present();
        Self {
            runtime,
            _directory: directory,
            history,
            controller,
            window,
        }
    }

    fn rows(&self) -> Vec<Entry> {
        self.runtime
            .block_on(self.history.search(SearchFilter::default()))
            .unwrap()
    }

    fn prompt(&self, target: Target) -> adw::Dialog {
        match target {
            Target::Selected => self.controller.emit(HistoryDialogInput::DeleteSelected),
            Target::All => self.controller.emit(HistoryDialogInput::ClearAllRequested),
            Target::Preferences => {
                let preferences = self.window.visible_dialog().unwrap();
                button(preferences.upcast_ref(), "Clear…").unwrap().emit_clicked();
            }
        }
        wait_until(|| {
            self.window
                .visible_dialog()
                .is_some_and(|dialog| dialog.is::<adw::AlertDialog>())
        });
        self.window.visible_dialog().unwrap()
    }
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn selected_history_confirmation_preserves_cancelled_rows_and_deletes_only_selected() {
    exercise(Target::Selected);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn clear_history_confirmation_preserves_cancelled_rows_and_clears_only_when_confirmed() {
    exercise(Target::All);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn preferences_history_confirmation_preserves_cancelled_rows_and_clears_only_when_confirmed() {
    exercise(Target::Preferences);
}

fn exercise(target: Target) {
    let fixture = Fixture::new();
    let clear = !matches!(target, Target::Selected);
    if matches!(target, Target::Preferences) {
        crate::ui::preferences::present(
            &fixture.window,
            Some(fixture.history.clone()),
            &crate::services::database_service::DatabaseService::new_isolated(),
            &crate::services::preferences::PreferencesStore::new(),
            None,
        );
    }
    wait_until(|| fixture.controller.model().entries.len() == 2);
    let selected = fixture.rows()[0].id;
    fixture
        .controller
        .emit(HistoryDialogInput::ToggleSelected(selected, true));
    wait_until(|| fixture.controller.model().selected_ids.contains(&selected));
    let dialog = fixture.prompt(target);
    dialog.force_close();
    wait_until(|| fixture.window.visible_dialog().as_ref() != Some(&dialog));
    assert_eq!(fixture.rows().len(), 2);
    let dialog = fixture.prompt(target);
    button(dialog.upcast_ref(), "Cancel").unwrap().emit_clicked();
    wait_until(|| fixture.window.visible_dialog().as_ref() != Some(&dialog));
    assert_eq!(fixture.rows().len(), 2);
    let dialog = fixture.prompt(target);
    button(dialog.upcast_ref(), if clear { "Clear" } else { "Delete" })
        .unwrap()
        .emit_clicked();
    wait_until(|| fixture.rows().len() == if clear { 0 } else { 1 });
    assert!(fixture.rows().iter().all(|row| row.id != selected));
    if !clear {
        assert!(fixture.rows()[0].pinned);
    }
    wait_until(|| fixture.window.visible_dialog().as_ref() != Some(&dialog));
    if let Some(preferences) = fixture.window.visible_dialog() {
        preferences.force_close();
    }
    fixture.window.close();
}

fn wait_until(predicate: impl Fn() -> bool) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        while context.iteration(false) {}
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(predicate());
}

fn button(widget: &gtk::Widget, label: &str) -> Option<gtk::Button> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>()
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
