use std::cell::RefCell;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use std::sync::Arc;

use tablepro_core::QueryResult;

use super::row_object::RowObject;

pub(super) enum Slot {
    /// A row that has not yet been requested from the shared result.
    Shared(usize),
    /// A weak cache for a shared row. The source index recreates it after
    /// the view and every other consumer release the current RowObject.
    Cached {
        source_index: usize,
        object: glib::WeakRef<RowObject>,
    },
    /// Drafts and replacement rows have no immutable source row to rebuild
    /// from, so the model remains their owner.
    Object(RowObject),
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct RowStore {
        pub(in crate::ui) slots: RefCell<Vec<Slot>>,
        pub(in crate::ui) source: RefCell<Option<Arc<QueryResult>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RowStore {
        const NAME: &'static str = "TableProRowStore";
        type Type = super::RowStore;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for RowStore {}

    impl ListModelImpl for RowStore {
        fn item_type(&self) -> glib::Type {
            RowObject::static_type()
        }

        fn n_items(&self) -> u32 {
            self.slots.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            let mut slots = self.slots.borrow_mut();
            let slot = slots.get_mut(position as usize)?;
            match slot {
                Slot::Shared(index) => {
                    let source_index = *index;
                    let source = self.source.borrow();
                    let cells = source.as_ref()?.rows.get(source_index)?.clone();
                    let row = RowObject::new(cells);
                    let object = glib::WeakRef::new();
                    object.set(Some(&row));
                    *slot = Slot::Cached { source_index, object };
                    Some(row.upcast())
                }
                Slot::Cached { source_index, object } => {
                    if let Some(row) = object.upgrade() {
                        Some(row.upcast())
                    } else {
                        let source_index = *source_index;
                        let source = self.source.borrow();
                        let cells = source.as_ref()?.rows.get(source_index)?.clone();
                        let row = RowObject::new(cells);
                        object.set(Some(&row));
                        Some(row.upcast())
                    }
                }
                Slot::Object(object) => Some(object.clone().upcast()),
            }
        }
    }
}

glib::wrapper! {
    pub struct RowStore(ObjectSubclass<imp::RowStore>) @implements gio::ListModel;
}

impl RowStore {
    pub fn from_shared(result: Arc<QueryResult>) -> Self {
        let store: Self = glib::Object::new();
        store.install_shared(result);
        store
    }

    pub fn replace_shared(&self, result: Arc<QueryResult>) {
        let removed = self.n_items();
        self.install_shared(result);
        self.items_changed(0, removed, self.n_items());
    }

    fn install_shared(&self, result: Arc<QueryResult>) {
        *self.imp().slots.borrow_mut() = (0..result.rows.len()).map(Slot::Shared).collect();
        *self.imp().source.borrow_mut() = Some(result);
    }

    pub fn insert(&self, position: u32, row: &RowObject) {
        self.splice(position, 0, std::slice::from_ref(row));
    }

    pub fn remove(&self, position: u32) {
        self.splice(position, 1, &[]);
    }

    pub fn splice(&self, position: u32, removals: u32, additions: &[RowObject]) {
        let removed = {
            let mut slots = self.imp().slots.borrow_mut();
            let start = (position as usize).min(slots.len());
            let end = (start + removals as usize).min(slots.len());
            let replacement = additions.iter().cloned().map(Slot::Object);
            slots.splice(start..end, replacement);
            (end - start) as u32
        };
        self.items_changed(position, removed, additions.len() as u32);
    }

    pub fn find(&self, row: &RowObject) -> Option<u32> {
        let slots = self.imp().slots.borrow();
        let position = slots.iter().position(|slot| match slot {
            Slot::Object(object) => object == row,
            Slot::Cached { object, .. } => object.upgrade().is_some_and(|cached| cached == *row),
            Slot::Shared(_) => false,
        })?;
        Some(position as u32)
    }

    #[cfg(test)]
    fn materialized(&self) -> usize {
        let slots = self.imp().slots.borrow();
        slots
            .iter()
            .filter(|slot| match slot {
                Slot::Object(_) => true,
                Slot::Cached { object, .. } => object.upgrade().is_some(),
                Slot::Shared(_) => false,
            })
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;
    use tablepro_core::Value;

    fn rows(count: i64) -> Vec<Vec<Value>> {
        (0..count).map(|n| vec![Value::Int(n)]).collect()
    }

    fn first_cell(store: &RowStore, position: u32) -> Value {
        let row = store.item(position).and_downcast::<RowObject>();
        row.map(|row| row.cell_value(0)).unwrap_or(Value::Null)
    }

    #[test]
    fn no_row_object_exists_until_a_row_is_requested() {
        let store = RowStore::from_shared(shared(1000));
        assert_eq!(store.n_items(), 1000);
        assert_eq!(store.materialized(), 0);
        let row = store.item(41).and_downcast::<RowObject>().unwrap();
        assert_eq!(row.cell_value(0), Value::Int(41));
        assert_eq!(store.materialized(), 1);
        drop(row);
        assert_eq!(store.materialized(), 0);
        assert_eq!(first_cell(&store, 41), Value::Int(41));
        assert_eq!(store.materialized(), 0);
    }

    #[test]
    fn a_requested_row_keeps_its_identity_and_its_edits() {
        let store = RowStore::from_shared(shared(3));
        let first = store.item(1).and_downcast::<RowObject>().unwrap();
        first.set_cell(0, Value::Int(99));
        let again = store.item(1).and_downcast::<RowObject>().unwrap();
        assert_eq!(first, again);
        assert_eq!(again.cell_value(0), Value::Int(99));
        assert_eq!(store.find(&again), Some(1));
    }

    #[test]
    fn a_released_clean_row_can_be_recreated_from_the_shared_result() {
        let store = RowStore::from_shared(shared(3));
        let first = store.item(1).and_downcast::<RowObject>().unwrap();
        let held_identity = first.clone();
        assert_eq!(store.materialized(), 1);
        drop(first);

        let again = store.item(1).and_downcast::<RowObject>().unwrap();
        assert_eq!(again, held_identity);
        assert_eq!(again.cell_value(0), Value::Int(1));
        assert_eq!(store.materialized(), 1);
        drop(held_identity);
        assert_eq!(store.materialized(), 1);
        drop(again);
        assert_eq!(store.materialized(), 0);

        let recreated = store.item(1).and_downcast::<RowObject>().unwrap();
        assert_eq!(recreated.cell_value(0), Value::Int(1));
        assert_eq!(store.materialized(), 1);
    }

    #[test]
    fn draft_rows_remain_owned_by_the_store() {
        let store = RowStore::from_shared(shared(1));
        let draft = RowObject::new_draft(7, vec![Value::Int(-1)]);
        store.insert(0, &draft);
        drop(draft);

        assert_eq!(store.materialized(), 1);
        let again = store.item(0).and_downcast::<RowObject>().unwrap();
        assert_eq!(again.draft_id(), Some(7));
        assert_eq!(again.cell_value(0), Value::Int(-1));
    }

    #[test]
    fn an_out_of_range_position_yields_nothing() {
        let store = RowStore::from_shared(shared(2));
        assert!(store.item(2).is_none());
    }

    #[test]
    fn insert_remove_and_splice_move_rows_and_report_the_change() {
        let store = RowStore::from_shared(shared(3));
        let changes = Rc::new(Cell::new((0, 0, 0)));
        let seen = changes.clone();
        store.connect_items_changed(move |_, position, removed, added| seen.set((position, removed, added)));

        store.insert(0, &RowObject::new_draft(7, vec![Value::Int(-1)]));
        assert_eq!(changes.get(), (0, 0, 1));
        assert_eq!(store.n_items(), 4);
        assert_eq!(first_cell(&store, 0), Value::Int(-1));
        assert_eq!(first_cell(&store, 1), Value::Int(0));

        store.remove(0);
        assert_eq!(changes.get(), (0, 1, 0));
        assert_eq!(first_cell(&store, 0), Value::Int(0));

        store.splice(1, 1, &[RowObject::new(vec![Value::Int(50)])]);
        assert_eq!(changes.get(), (1, 1, 1));
        assert_eq!(first_cell(&store, 1), Value::Int(50));
        assert_eq!(first_cell(&store, 2), Value::Int(2));
    }

    #[test]
    fn replacing_the_rows_reports_one_change_and_drops_old_objects() {
        let store = RowStore::from_shared(shared(4));
        let held = store.item(0).and_downcast::<RowObject>().unwrap();
        let changes = Rc::new(Cell::new((0, 0, 0)));
        let seen = changes.clone();
        store.connect_items_changed(move |_, position, removed, added| seen.set((position, removed, added)));

        store.replace_shared(shared(2));
        assert_eq!(changes.get(), (0, 4, 2));
        assert_eq!(store.n_items(), 2);
        assert_eq!(store.materialized(), 0);
        assert_eq!(store.find(&held), None);
    }

    #[test]
    fn a_handler_may_read_the_model_while_it_is_changing() {
        let store = RowStore::from_shared(shared(2));
        let reader = store.clone();
        store.connect_items_changed(move |_, _, _, _| {
            let _ = reader.item(0);
        });
        store.replace_shared(shared(3));
        store.remove(0);
        assert_eq!(store.n_items(), 2);
    }

    fn shared(count: i64) -> Arc<QueryResult> {
        Arc::new(QueryResult {
            columns: Vec::new(),
            rows: rows(count),
            truncated: false,
        })
    }

    #[test]
    fn a_shared_result_is_read_in_place_and_never_copied_up_front() {
        let result = shared(500);
        let store = RowStore::from_shared(result.clone());
        assert_eq!(store.n_items(), 500);
        assert_eq!(store.materialized(), 0);
        assert_eq!(Arc::strong_count(&result), 2);
        let row = store.item(499).and_downcast::<RowObject>().unwrap();
        assert_eq!(row.cell_value(0), Value::Int(499));
        assert_eq!(store.materialized(), 1);
    }

    #[test]
    fn shared_rows_survive_a_draft_inserted_above_them() {
        let store = RowStore::from_shared(shared(3));
        store.insert(0, &RowObject::new_draft(1, vec![Value::Int(-1)]));
        assert_eq!(first_cell(&store, 0), Value::Int(-1));
        assert_eq!(first_cell(&store, 3), Value::Int(2));
        store.remove(0);
        assert_eq!(first_cell(&store, 0), Value::Int(0));
    }

    #[test]
    fn replacing_a_shared_result_releases_the_old_one() {
        let old = shared(10);
        let store = RowStore::from_shared(old.clone());
        store.replace_shared(shared(2));
        assert_eq!(Arc::strong_count(&old), 1);
        assert_eq!(store.n_items(), 2);
    }
}
