pub use bookie_app_services::structure_tracker::*;

use std::cell::RefCell;
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct WorkspaceStructureTrackers(std::rc::Rc<RefCell<StructureTrackerRegistry>>);

impl WorkspaceStructureTrackers {
    pub fn open_tab(&self, id: Uuid) {
        self.0.borrow_mut().open_tab(id);
    }
    pub fn close_tab(&self, id: Uuid) {
        self.0.borrow_mut().close_tab(id);
    }
    pub fn with_tab<R>(&self, id: Uuid, f: impl FnOnce(&mut StructureChangeTracker) -> R) -> R {
        self.0.borrow_mut().with_tab(id, f)
    }
    pub fn with_tab_ref<R>(&self, id: Uuid, f: impl FnOnce(&StructureChangeTracker) -> R) -> Option<R> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_stores_isolate_and_release_their_trackers() {
        let id = Uuid::new_v4();
        let first = WorkspaceStructureTrackers::default();
        let second = WorkspaceStructureTrackers::default();
        first.with_tab(id, |tracker| tracker.set_ops(vec![pending_op()]));
        assert!(first.with_tab_ref(id, |tracker| tracker.has_pending()).unwrap());
        assert!(second.with_tab_ref(id, |tracker| tracker.has_pending()).is_none());
        let shared = first.clone();
        drop(first);
        assert!(shared.with_tab_ref(id, |tracker| tracker.has_pending()).unwrap());
        shared.close_tab(id);
        assert!(shared.with_tab_ref(id, |_| ()).is_none());
    }

    #[test]
    fn closing_a_tab_removes_its_tracker() {
        let store = WorkspaceStructureTrackers::default();
        let id = Uuid::new_v4();
        store.open_tab(id);
        assert!(store.with_tab_ref(id, |_| ()).is_some());
        store.close_tab(id);
        assert!(store.with_tab_ref(id, |_| ()).is_none());
    }
    fn pending_op() -> bookie_core::sql_ddl::StructureOp {
        bookie_core::sql_ddl::StructureOp::AddColumn {
            schema: None,
            table: "t".into(),
            column: bookie_core::sql_ddl::DraftColumn {
                original: None,
                name: "a".into(),
                data_type: "int".into(),
                nullable: true,
                primary_key: false,
                auto_increment: false,
                default_value: None,
                comment: None,
                collation: None,
            },
        }
    }

    #[test]
    fn the_module_functions_track_a_tab_from_open_to_close() {
        let store = WorkspaceStructureTrackers::default();
        let dirty = Uuid::new_v4();
        let clean = Uuid::new_v4();
        let window: std::collections::HashSet<Uuid> = [dirty, clean].into();
        store.open_tab(clean);
        store.with_tab(dirty, |t| t.set_ops(vec![pending_op()]));
        assert_eq!(store.with_tab_ref(clean, |t| t.has_pending()), Some(false));
        assert_eq!(store.with_tab_ref(dirty, |t| t.has_pending()), Some(true));
        assert!(store.any_pending_in([clean, dirty]));
        assert!(!store.any_pending_in([clean]));
        assert_eq!(store.pending_among([clean, dirty]), vec![dirty]);
        assert!(store.pending_among([clean]).is_empty());
        assert_eq!(store.pending_tabs_for(&window), vec![dirty]);
        assert!(store.pending_tabs_for(&[clean].into()).is_empty());
        store.close_tab(dirty);
        assert_eq!(store.with_tab_ref(dirty, |t| t.has_pending()), None);
        assert!(!store.any_pending_in([clean, dirty]));
        assert!(store.pending_tabs_for(&window).is_empty());
    }
}
