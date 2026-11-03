pub use bookie_app_services::change_tracker::*;

use std::cell::RefCell;
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct WorkspaceChangeTrackers(std::rc::Rc<RefCell<ChangeTrackerRegistry>>);

impl WorkspaceChangeTrackers {
    pub fn open_tab(&self, id: Uuid) {
        self.0.borrow_mut().open_tab(id);
    }
    pub fn close_tab(&self, id: Uuid) {
        self.0.borrow_mut().close_tab(id);
    }
    pub fn with_tab<R>(&self, id: Uuid, f: impl FnOnce(&mut TabChangeTracker) -> R) -> Option<R> {
        self.0.borrow_mut().with_tab(id, f)
    }
    pub fn with_tab_ref<R>(&self, id: Uuid, f: impl FnOnce(&TabChangeTracker) -> R) -> Option<R> {
        self.0.borrow().with_tab_ref(id, f)
    }
    pub fn any_pending_in(&self, ids: impl IntoIterator<Item = Uuid>) -> bool {
        self.0.borrow().any_pending_in(ids)
    }
    pub fn pending_among(&self, ids: impl IntoIterator<Item = Uuid>) -> Vec<Uuid> {
        self.0.borrow().pending_among(ids)
    }
    pub fn pending_tabs_for(&self, ids: &std::collections::HashSet<Uuid>) -> Vec<Uuid> {
        self.0.borrow().pending_tabs_for(ids)
    }
    pub fn subscribe(&self, id: Uuid, sender: relm4::Sender<TrackerEvent>) {
        self.with_tab(id, |tracker| tracker.subscribe(RelmTrackerEvents(sender)));
    }
}

#[derive(Debug)]
struct RelmTrackerEvents(relm4::Sender<TrackerEvent>);

impl TrackerEventSink for RelmTrackerEvents {
    fn emit(&self, event: TrackerEvent) {
        let _ = self.0.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::Value;

    #[test]
    fn workspace_trackers_keep_edits_separate_and_release_them() {
        let id = uuid::Uuid::new_v4();
        let first = WorkspaceChangeTrackers::default();
        let second = WorkspaceChangeTrackers::default();
        first.open_tab(id);
        second.open_tab(id);
        first.with_tab(id, |tracker| {
            tracker.track_insert(vec![Value::Int(4)]);
        });
        assert!(first.any_pending_in([id]));
        assert!(!second.any_pending_in([id]));
        let shared = first.clone();
        let weak = std::rc::Rc::downgrade(&first.0);
        drop(first);
        assert!(shared.any_pending_in([id]));
        shared.close_tab(id);
        assert!(shared.with_tab_ref(id, |_| ()).is_none());
        assert!(
            shared
                .with_tab(id, |tracker| tracker.track_insert(vec![Value::Int(5)]))
                .is_none()
        );
        assert!(!shared.any_pending_in([id]));
        assert!(second.with_tab_ref(id, |_| ()).is_some());
        drop(shared);
        assert!(weak.upgrade().is_none());
    }

    #[tokio::test]
    async fn a_deleted_rows_unique_value_can_be_added_again_in_the_same_save() {
        use bookie_core::{ConnectOptions, DatabaseDriver, OperationControl};
        let connection = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: ":memory:".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let control = OperationControl::with_timeout(std::time::Duration::from_secs(10));
        for sql in [
            "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL UNIQUE)",
            "INSERT INTO users VALUES (1, 'a@example.com')",
        ] {
            connection.execute_controlled(sql, &control).await.unwrap();
        }
        let columns = connection
            .fetch_columns_controlled(None, "users", &control)
            .await
            .unwrap();
        let mut tracker = TabChangeTracker::new();
        tracker.track_delete(
            RowKey::from_pk_values(&[Value::Int(1)]).unwrap(),
            vec![Value::Int(1), Value::Text("a@example.com".into())],
        );
        tracker.track_insert(vec![Value::Int(2), Value::Text("a@example.com".into())]);
        let (statements, _) = tracker.materialize("sqlite", None, "users", &columns).unwrap();

        connection
            .execute_in_transaction_controlled(&statements, &control)
            .await
            .expect("the delete frees the unique email before the insert takes it");
        let rows = connection
            .query_controlled("SELECT email FROM users", &control)
            .await
            .unwrap()
            .rows;
        assert_eq!(rows, vec![vec![Value::Text("a@example.com".into())]]);
    }

    #[tokio::test]
    async fn the_app_bridge_delivers_row_and_pending_count_events_to_relm() {
        let store = WorkspaceChangeTrackers::default();
        let id = uuid::Uuid::new_v4();
        let (sender, receiver) = relm4::channel();
        store.open_tab(id);
        store.subscribe(id, sender);
        let key = RowKey::from_pk_values(&[Value::Int(7)]).unwrap();
        store.with_tab(id, |tracker| {
            tracker.track_cell_edit(key.clone(), 1, Value::Int(1), Value::Int(2))
        });
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), receiver.recv())
            .await
            .unwrap();
        assert!(matches!(event, Some(TrackerEvent::ChangedRows(keys)) if keys == vec![key]));
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), receiver.recv())
            .await
            .unwrap();
        assert!(matches!(event, Some(TrackerEvent::PendingCountChanged(1))));
        store.close_tab(id);
        assert!(store.with_tab_ref(id, |_| ()).is_none());
    }
}
