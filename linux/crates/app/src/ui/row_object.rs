use std::cell::{Cell, RefCell};

use gtk4::glib;
use gtk4::subclass::prelude::*;

use tablepro_core::Value;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct RowObject {
        pub cells: RefCell<Vec<Option<Value>>>,
        /// Local id for draft (uninserted) rows added via the
        /// inline-Insert flow. `None` for persisted rows fetched
        /// from the database. Lets `connect_bind` distinguish a
        /// draft (which needs the green-border CSS + RowKey::Draft
        /// tracker lookup) from a persisted row whose PK columns
        /// happen to be NULL.
        pub draft_id: Cell<Option<u64>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RowObject {
        const NAME: &'static str = "TableProRowObject";
        type Type = super::RowObject;
    }

    impl ObjectImpl for RowObject {}
}

glib::wrapper! {
    pub struct RowObject(ObjectSubclass<imp::RowObject>);
}

impl RowObject {
    pub fn new(cells: Vec<Value>) -> Self {
        let obj: Self = glib::Object::new();
        *obj.imp().cells.borrow_mut() = cells.into_iter().map(Some).collect();
        obj
    }

    pub fn new_projected(cells: Vec<Value>, indices: &[usize], width: usize) -> Option<Self> {
        if cells.len() != indices.len() {
            return None;
        }
        let mut values = vec![None; width];
        for (index, value) in indices.iter().copied().zip(cells) {
            if index >= width || values[index].is_some() {
                return None;
            }
            values[index] = Some(value);
        }
        let obj: Self = glib::Object::new();
        *obj.imp().cells.borrow_mut() = values;
        Some(obj)
    }

    pub fn new_draft(draft_id: u64, cells: Vec<Value>) -> Self {
        let obj = Self::new(cells);
        obj.imp().draft_id.set(Some(draft_id));
        obj
    }

    pub fn draft_id(&self) -> Option<u64> {
        self.imp().draft_id.get()
    }

    pub fn cell_value(&self, idx: usize) -> Value {
        self.imp()
            .cells
            .borrow()
            .get(idx)
            .cloned()
            .flatten()
            .unwrap_or_else(|| Value::Undecodable("not fetched".into()))
    }

    pub fn cell_is_loaded(&self, idx: usize) -> bool {
        self.imp().cells.borrow().get(idx).is_some_and(Option::is_some)
    }

    pub fn cells_are_complete(&self) -> bool {
        self.imp().cells.borrow().iter().all(Option::is_some)
    }

    pub fn complete_cells(&self) -> Option<Vec<Value>> {
        self.imp().cells.borrow().iter().cloned().collect()
    }

    pub fn loaded_cells(&self) -> Vec<Option<Value>> {
        self.imp().cells.borrow().clone()
    }

    pub fn clone_preserving_loading_state(&self) -> Self {
        let row: Self = glib::Object::new();
        *row.imp().cells.borrow_mut() = self.loaded_cells();
        row.imp().draft_id.set(self.draft_id());
        row
    }

    pub fn cells_clone(&self) -> Vec<Value> {
        self.imp()
            .cells
            .borrow()
            .iter()
            .cloned()
            .map(|value| value.unwrap_or_else(|| Value::Undecodable("not fetched".into())))
            .collect()
    }

    pub fn with_cells<R>(&self, f: impl FnOnce(&[Value]) -> R) -> R {
        let cells = self
            .imp()
            .cells
            .borrow()
            .iter()
            .cloned()
            .map(|value| value.unwrap_or_else(|| Value::Undecodable("not fetched".into())))
            .collect::<Vec<_>>();
        f(&cells)
    }

    /// In-place cell mutation. Used by the inline-edit flow on draft
    /// rows so the grid renders the user's typed value immediately
    /// instead of waiting for a re-fetch.
    pub fn set_cell(&self, idx: usize, value: Value) {
        let mut cells = self.imp().cells.borrow_mut();
        if idx < cells.len() {
            cells[idx] = Some(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_cells_distinguish_unfetched_values_from_sql_null() {
        let row = RowObject::new_projected(vec![Value::Null, Value::Text("shown".into())], &[0, 2], 3).unwrap();

        assert!(row.cell_is_loaded(0));
        assert_eq!(row.cell_value(0), Value::Null);
        assert!(!row.cell_is_loaded(1));
        assert_eq!(row.cell_value(1), Value::Undecodable("not fetched".into()));
        assert!(!row.cells_are_complete());
        assert_eq!(row.complete_cells(), None);
    }

    #[test]
    fn projected_cells_refuse_mismatched_or_duplicate_mappings() {
        assert!(RowObject::new_projected(vec![Value::Int(1)], &[], 1).is_none());
        assert!(RowObject::new_projected(vec![Value::Int(1), Value::Int(2)], &[0, 0], 1).is_none());
    }

    #[test]
    fn cloning_a_projected_row_keeps_unfetched_cells_unloaded() {
        let row = RowObject::new_projected(vec![Value::Int(1)], &[0], 2).unwrap();
        let clone = row.clone_preserving_loading_state();

        assert_eq!(clone.cell_value(0), Value::Int(1));
        assert!(!clone.cell_is_loaded(1));
        assert!(clone.complete_cells().is_none());
    }
}
