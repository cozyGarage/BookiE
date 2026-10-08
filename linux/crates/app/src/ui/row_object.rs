use std::cell::{Cell, RefCell};

use gtk4::glib;
use gtk4::subclass::prelude::*;

use tablepro_core::Value;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct RowObject {
        pub cells: RefCell<Vec<Option<Value>>>,
        pub previews: RefCell<Vec<Option<CellPreview>>>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct CellPreview {
    pub value: Value,
    pub byte_count: usize,
}

impl RowObject {
    pub fn new(cells: Vec<Value>) -> Self {
        let obj: Self = glib::Object::new();
        *obj.imp().cells.borrow_mut() = cells.into_iter().map(Some).collect();
        *obj.imp().previews.borrow_mut() = vec![None; obj.imp().cells.borrow().len()];
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
        *obj.imp().previews.borrow_mut() = vec![None; width];
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

    pub fn cell_preview(&self, idx: usize) -> Option<CellPreview> {
        self.imp().previews.borrow().get(idx).cloned().flatten()
    }

    pub fn preview_long_values(&self, key_indices: &[usize]) {
        let mut cells = self.imp().cells.borrow_mut();
        let mut previews = self.imp().previews.borrow_mut();
        if key_indices.is_empty()
            || key_indices.iter().any(|&index| {
                cells
                    .get(index)
                    .and_then(Option::as_ref)
                    .is_none_or(|value| matches!(value, Value::Undecodable(_)))
            })
        {
            return;
        }
        for (index, cell) in cells.iter_mut().enumerate() {
            if key_indices.contains(&index) {
                continue;
            }
            let Some(value) = cell.as_ref() else { continue };
            if let Some(preview) = preview_value(value) {
                *cell = None;
                previews[index] = Some(preview);
            }
        }
    }

    pub fn preview_values_from_result(&self) {
        let mut cells = self.imp().cells.borrow_mut();
        let mut previews = self.imp().previews.borrow_mut();
        for (index, cell) in cells.iter_mut().enumerate() {
            let Some(value) = cell.as_ref() else { continue };
            if let Some(preview) = preview_value(value) {
                *cell = None;
                previews[index] = Some(preview);
            }
        }
    }

    pub fn preview_long_redis_string_values(&self, key_indices: &[usize]) {
        if self.cell_value(1) == Value::Text("string".into()) {
            self.preview_long_values(key_indices);
        }
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
        *row.imp().previews.borrow_mut() = self.imp().previews.borrow().clone();
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
            self.imp().previews.borrow_mut()[idx] = None;
        }
    }
}

fn preview_value(value: &Value) -> Option<CellPreview> {
    const PREVIEW_BYTES: usize = 8 * 1024;
    match value {
        Value::Text(text) if text.len() > PREVIEW_BYTES => Some(CellPreview {
            value: Value::Text(text[..text.floor_char_boundary(PREVIEW_BYTES)].to_owned()),
            byte_count: text.len(),
        }),
        Value::Json(json) => {
            let text = json.to_string();
            (text.len() > PREVIEW_BYTES).then(|| CellPreview {
                value: Value::Text(text[..text.floor_char_boundary(PREVIEW_BYTES)].to_owned()),
                byte_count: text.len(),
            })
        }
        Value::Bytes(bytes) if bytes.len() > PREVIEW_BYTES => Some(CellPreview {
            value: Value::Bytes(bytes[..PREVIEW_BYTES].to_vec()),
            byte_count: bytes.len(),
        }),
        _ => None,
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

    #[test]
    fn long_cells_keep_an_eight_kib_preview_and_primary_keys_remain_loaded() {
        let text = format!("{}é", "x".repeat(8191));
        let row = RowObject::new(vec![Value::Int(7), Value::Text(text.clone())]);
        row.preview_long_values(&[0]);

        assert_eq!(row.cell_value(0), Value::Int(7));
        assert_eq!(row.cell_value(1), Value::Undecodable("not fetched".into()));
        let preview = row.cell_preview(1).unwrap();
        assert_eq!(preview.value, Value::Text("x".repeat(8191)));
        assert_eq!(preview.byte_count, text.len());
        assert!(row.complete_cells().is_none());
    }

    #[test]
    fn previews_keep_the_byte_boundary_and_cover_json_and_binary_values() {
        let exact = RowObject::new(vec![Value::Int(1), Value::Text("x".repeat(8192))]);
        exact.preview_long_values(&[0]);
        assert!(exact.cell_is_loaded(1));

        let bytes = vec![0xA5; 8193];
        let row = RowObject::new(vec![
            Value::Int(1),
            Value::Bytes(bytes.clone()),
            Value::Json(serde_json::json!("x".repeat(8192))),
        ]);
        row.preview_long_values(&[0]);
        assert_eq!(row.cell_preview(1).unwrap().value, Value::Bytes(bytes[..8192].to_vec()));
        assert_eq!(row.cell_preview(1).unwrap().byte_count, 8193);
        assert!(row.cell_preview(2).is_some());
    }
}
