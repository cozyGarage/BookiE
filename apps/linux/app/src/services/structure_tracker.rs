pub use bookie_app_services::structure_tracker::*;

use std::cell::RefCell;
use uuid::Uuid;

thread_local! {
    static REGISTRY: RefCell<StructureTrackerRegistry> = RefCell::new(StructureTrackerRegistry::default());
}

pub fn open_tab(tab_id: Uuid) {
    REGISTRY.with(|r| r.borrow_mut().open_tab(tab_id));
}

pub fn close_tab(tab_id: Uuid) {
    REGISTRY.with(|r| r.borrow_mut().close_tab(tab_id));
}

pub fn with_tab<F, R>(tab_id: Uuid, f: F) -> R
where
    F: FnOnce(&mut StructureChangeTracker) -> R,
{
    REGISTRY.with(|r| r.borrow_mut().with_tab(tab_id, f))
}

pub fn with_tab_ref<F, R>(tab_id: Uuid, f: F) -> Option<R>
where
    F: FnOnce(&StructureChangeTracker) -> R,
{
    REGISTRY.with(|r| r.borrow().with_tab_ref(tab_id, f))
}

pub fn any_pending_in(ids: impl IntoIterator<Item = Uuid>) -> bool {
    REGISTRY.with(|r| r.borrow().any_pending_in(ids))
}

pub fn pending_among(ids: impl IntoIterator<Item = Uuid>) -> Vec<Uuid> {
    REGISTRY.with(|r| r.borrow().pending_among(ids))
}

pub fn pending_tabs_for(tab_ids: &std::collections::HashSet<Uuid>) -> Vec<Uuid> {
    REGISTRY.with(|r| r.borrow().pending_tabs_for(tab_ids))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_a_tab_removes_its_tracker() {
        let id = Uuid::new_v4();
        open_tab(id);
        assert!(with_tab_ref(id, |_| ()).is_some());
        close_tab(id);
        assert!(with_tab_ref(id, |_| ()).is_none());
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
        let dirty = Uuid::new_v4();
        let clean = Uuid::new_v4();
        let window: std::collections::HashSet<Uuid> = [dirty, clean].into();
        open_tab(clean);
        with_tab(dirty, |t| t.set_ops(vec![pending_op()]));
        assert_eq!(with_tab_ref(clean, |t| t.has_pending()), Some(false));
        assert_eq!(with_tab_ref(dirty, |t| t.has_pending()), Some(true));
        assert!(any_pending_in([clean, dirty]));
        assert!(!any_pending_in([clean]));
        assert_eq!(pending_among([clean, dirty]), vec![dirty]);
        assert!(pending_among([clean]).is_empty());
        assert_eq!(pending_tabs_for(&window), vec![dirty]);
        assert!(pending_tabs_for(&[clean].into()).is_empty());
        close_tab(dirty);
        assert_eq!(with_tab_ref(dirty, |t| t.has_pending()), None);
        assert!(!any_pending_in([clean, dirty]));
        assert!(pending_tabs_for(&window).is_empty());
    }
}
