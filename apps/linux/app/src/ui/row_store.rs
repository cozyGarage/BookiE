use std::cell::RefCell;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use std::sync::Arc;

use tablepro_core::{QueryResult, Value};

use crate::services::change_tracker::{KeyValue, RowKey};

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
        pub(in crate::ui) projection: RefCell<Option<(Vec<usize>, usize)>>,
        pub(in crate::ui) preview_keys: RefCell<Vec<usize>>,
        pub(in crate::ui) preview_redis_strings: std::cell::Cell<bool>,
        pub(in crate::ui) preview_from_result: std::cell::Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RowStore {
        const NAME: &str = "TableProRowStore";
        type Type = super::RowStore;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for RowStore {
        fn dispose(&self) {
            self.obj().detach_cached_rows();
        }
    }

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
                    let result = source.as_ref()?;
                    let projection = self.projection.borrow();
                    let row = row_for_source(
                        result,
                        source_index,
                        projection.as_ref(),
                        &self.preview_keys.borrow(),
                        self.preview_redis_strings.get(),
                        self.preview_from_result.get(),
                    )?;
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
                        let result = source.as_ref()?;
                        let projection = self.projection.borrow();
                        let row = row_for_source(
                            result,
                            source_index,
                            projection.as_ref(),
                            &self.preview_keys.borrow(),
                            self.preview_redis_strings.get(),
                            self.preview_from_result.get(),
                        )?;
                        object.set(Some(&row));
                        Some(row.upcast())
                    }
                }
                Slot::Object(object) => Some(object.clone().upcast()),
            }
        }
    }
}

fn row_for_source(
    result: &Arc<QueryResult>,
    source_index: usize,
    projection: Option<&(Vec<usize>, usize)>,
    preview_keys: &[usize],
    preview_redis_strings: bool,
    preview_from_result: bool,
) -> Option<RowObject> {
    let row = RowObject::from_shared(result, source_index, projection.cloned())?;
    if preview_from_result {
        row.preview_values_from_result();
    } else if preview_redis_strings {
        row.preview_long_redis_string_values(preview_keys);
    } else {
        row.preview_long_values(preview_keys);
    }
    Some(row)
}

fn valid_projection(result: &QueryResult, indices: &[usize], width: usize) -> bool {
    (indices.len() == result.columns.len() || (result.columns.is_empty() && result.rows.is_empty()))
        && indices.iter().all(|index| *index < width)
        && indices.iter().copied().collect::<std::collections::HashSet<_>>().len() == indices.len()
        && result.rows.iter().all(|row| row.len() == indices.len())
}

glib::wrapper! {
    pub struct RowStore(ObjectSubclass<imp::RowStore>) @implements gio::ListModel;
}

impl RowStore {
    pub(super) fn visit_search_cells(&self, position: u32, visit: impl FnMut(usize, &Value) -> bool) -> bool {
        let slots = self.imp().slots.borrow();
        let Some(slot) = slots.get(position as usize) else {
            return false;
        };
        match slot {
            Slot::Object(row) => row.visit_cells(visit),
            Slot::Shared(source_index) => self.visit_source_cells(*source_index, visit),
            Slot::Cached { source_index, object } => {
                if let Some(row) = object.upgrade() {
                    return row.visit_cells(visit);
                }
                self.visit_source_cells(*source_index, visit)
            }
        }
    }

    fn visit_source_cells(&self, source_index: usize, mut visit: impl FnMut(usize, &Value) -> bool) -> bool {
        let source = self.imp().source.borrow();
        let Some(cells) = source.as_ref().and_then(|result| result.rows.get(source_index)) else {
            return false;
        };
        let projection = self.imp().projection.borrow();
        if let Some((indices, _)) = projection.as_ref() {
            for (source_index, value) in cells.iter().enumerate() {
                if indices.get(source_index).is_some_and(|column| visit(*column, value)) {
                    return true;
                }
            }
        } else {
            for (column, value) in cells.iter().enumerate() {
                if visit(column, value) {
                    return true;
                }
            }
        }
        false
    }

    fn source_cell(&self, source_index: usize, column: usize) -> Option<Value> {
        let source = self.imp().source.borrow();
        let result = source.as_ref()?;
        let cells = result.rows.get(source_index)?;
        let projection = self.imp().projection.borrow();
        let source_column = match projection.as_ref() {
            Some((indices, _)) => indices.iter().position(|index| *index == column)?,
            None => column,
        };
        cells.get(source_column).cloned()
    }

    pub fn from_shared(result: Arc<QueryResult>) -> Self {
        let store: Self = glib::Object::new();
        store.install_shared(result, None, Vec::new());
        store
    }

    pub fn from_shared_with_previews(result: Arc<QueryResult>, key_indices: Vec<usize>) -> Self {
        let store: Self = glib::Object::new();
        store.install_shared(result, None, key_indices);
        store
    }

    pub fn from_shared_with_result_previews(result: Arc<QueryResult>) -> Self {
        let store: Self = glib::Object::new();
        store.install_shared(result, None, Vec::new());
        store.imp().preview_from_result.set(true);
        store
    }

    pub fn from_shared_with_redis_string_previews(result: Arc<QueryResult>, key_indices: Vec<usize>) -> Self {
        let store: Self = glib::Object::new();
        store.install_shared(result, None, key_indices);
        store.imp().preview_redis_strings.set(true);
        store
    }

    pub fn from_projected(result: Arc<QueryResult>, indices: Vec<usize>, width: usize) -> Result<Self, String> {
        if !valid_projection(&result, &indices, width) {
            return Err("projected browse result does not match its schema mapping".into());
        }
        let store: Self = glib::Object::new();
        store.install_shared(result, Some((indices, width)), Vec::new());
        Ok(store)
    }

    pub fn from_projected_with_previews(
        result: Arc<QueryResult>,
        indices: Vec<usize>,
        width: usize,
        key_indices: Vec<usize>,
    ) -> Result<Self, String> {
        if !valid_projection(&result, &indices, width) {
            return Err("projected browse result does not match its schema mapping".into());
        }
        let store: Self = glib::Object::new();
        store.install_shared(result, Some((indices, width)), key_indices);
        Ok(store)
    }

    pub fn from_projected_with_redis_string_previews(
        result: Arc<QueryResult>,
        indices: Vec<usize>,
        width: usize,
        key_indices: Vec<usize>,
    ) -> Result<Self, String> {
        let store = Self::from_projected_with_previews(result, indices, width, key_indices)?;
        store.imp().preview_redis_strings.set(true);
        Ok(store)
    }

    pub fn replace_shared(&self, result: Arc<QueryResult>) {
        self.replace_shared_with_previews(result, Vec::new());
    }

    pub fn replace_shared_with_previews(&self, result: Arc<QueryResult>, key_indices: Vec<usize>) {
        let removed = self.n_items();
        self.install_shared(result, None, key_indices);
        self.items_changed(0, removed, self.n_items());
    }

    pub fn replace_shared_with_redis_string_previews(&self, result: Arc<QueryResult>, key_indices: Vec<usize>) {
        let removed = self.n_items();
        self.install_shared(result, None, key_indices);
        self.imp().preview_redis_strings.set(true);
        self.items_changed(0, removed, self.n_items());
    }

    pub fn replace_projected(&self, result: Arc<QueryResult>, indices: Vec<usize>, width: usize) -> Result<(), String> {
        self.replace_projected_with_previews(result, indices, width, Vec::new())
    }

    pub fn replace_projected_with_previews(
        &self,
        result: Arc<QueryResult>,
        indices: Vec<usize>,
        width: usize,
        key_indices: Vec<usize>,
    ) -> Result<(), String> {
        if !valid_projection(&result, &indices, width) {
            return Err("projected browse result does not match its schema mapping".into());
        }
        let removed = self.n_items();
        self.install_shared(result, Some((indices, width)), key_indices);
        self.items_changed(0, removed, self.n_items());
        Ok(())
    }

    pub fn replace_projected_with_redis_string_previews(
        &self,
        result: Arc<QueryResult>,
        indices: Vec<usize>,
        width: usize,
        key_indices: Vec<usize>,
    ) -> Result<(), String> {
        self.replace_projected_with_previews(result, indices, width, key_indices)?;
        self.imp().preview_redis_strings.set(true);
        Ok(())
    }

    fn install_shared(
        &self,
        result: Arc<QueryResult>,
        projection: Option<(Vec<usize>, usize)>,
        preview_keys: Vec<usize>,
    ) {
        self.detach_cached_rows();
        *self.imp().slots.borrow_mut() = (0..result.rows.len()).map(Slot::Shared).collect();
        *self.imp().source.borrow_mut() = Some(result);
        *self.imp().projection.borrow_mut() = projection;
        *self.imp().preview_keys.borrow_mut() = preview_keys;
        self.imp().preview_redis_strings.set(false);
        self.imp().preview_from_result.set(false);
    }

    fn detach_cached_rows(&self) {
        for slot in self.imp().slots.borrow().iter() {
            match slot {
                Slot::Cached { object, .. } => {
                    if let Some(row) = object.upgrade() {
                        row.detach_shared();
                    }
                }
                Slot::Object(row) => row.detach_shared(),
                Slot::Shared(_) => {}
            }
        }
    }

    pub fn source_value_at(&self, position: u32, column: usize) -> Option<tablepro_core::Value> {
        let source_index = {
            let slots = self.imp().slots.borrow();
            match slots.get(position as usize)? {
                Slot::Shared(index)
                | Slot::Cached {
                    source_index: index, ..
                } => *index,
                Slot::Object(_) => return None,
            }
        };
        self.source_cell(source_index, column)
    }

    pub fn source_index_at(&self, position: u32) -> Option<usize> {
        match self.imp().slots.borrow().get(position as usize)? {
            Slot::Shared(index)
            | Slot::Cached {
                source_index: index, ..
            } => Some(*index),
            Slot::Object(_) => None,
        }
    }

    pub fn reorder_source_rows(&self, order: &[usize]) -> bool {
        let count = self.n_items() as usize;
        if order.len() != count
            || order.iter().any(|index| *index >= count)
            || order.iter().copied().collect::<std::collections::HashSet<_>>().len() != count
            || self
                .imp()
                .slots
                .borrow()
                .iter()
                .any(|slot| matches!(slot, Slot::Object(_)))
        {
            return false;
        }
        self.replace_source_rows(order)
    }

    pub fn replace_source_rows(&self, source_indices: &[usize]) -> bool {
        let Some(source_count) = self.imp().source.borrow().as_ref().map(|result| result.rows.len()) else {
            return false;
        };
        if source_indices.iter().any(|index| *index >= source_count)
            || source_indices
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != source_indices.len()
            || self
                .imp()
                .slots
                .borrow()
                .iter()
                .any(|slot| matches!(slot, Slot::Object(_)))
        {
            return false;
        }
        let removed = self.n_items();
        self.detach_cached_rows();
        *self.imp().slots.borrow_mut() = source_indices.iter().copied().map(Slot::Shared).collect();
        self.items_changed(0, removed, source_indices.len() as u32);
        true
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

    pub fn position_of_key(&self, key: &RowKey, pk_indices: &[usize]) -> Option<u32> {
        (0..self.n_items()).find(|&position| match key {
            RowKey::Draft(id) => self.draft_id_at(position) == Some(*id),
            RowKey::Persisted(target) => {
                self.draft_id_at(position).is_none()
                    && !pk_indices.is_empty()
                    && pk_indices
                        .iter()
                        .map(|&column| {
                            let value = self.cell_at(position, column);
                            KeyValue::from(&value.unwrap_or_else(|| Value::Undecodable("not fetched".into())))
                        })
                        .eq(target.iter().cloned())
            }
        })
    }

    pub fn row_key_at(&self, position: u32, pk_indices: &[usize]) -> Option<RowKey> {
        if let Some(id) = self.draft_id_at(position) {
            return Some(RowKey::Draft(id));
        }
        let values = pk_indices
            .iter()
            .map(|&column| self.cell_at(position, column))
            .collect::<Option<Vec<_>>>()?;
        RowKey::from_pk_values(&values)
    }

    fn draft_id_at(&self, position: u32) -> Option<u64> {
        match self.imp().slots.borrow().get(position as usize)? {
            Slot::Object(row) => row.draft_id(),
            Slot::Shared(_) | Slot::Cached { .. } => None,
        }
    }

    fn cell_at(&self, position: u32, column: usize) -> Option<Value> {
        let source_index = match self.imp().slots.borrow().get(position as usize)? {
            Slot::Object(row) => return row.cell_value_in_row(column),
            Slot::Cached { source_index, object } => match object.upgrade() {
                Some(row) => return row.cell_value_in_row(column),
                None => *source_index,
            },
            Slot::Shared(index) => *index,
        };
        let source = self.imp().source.borrow();
        let cells = source.as_ref()?.rows.get(source_index)?;
        let projection = self.imp().projection.borrow();
        let (width, source_column) = match projection.as_ref() {
            Some((indices, width)) => (*width, indices.iter().position(|mapped| *mapped == column)),
            None => (cells.len(), Some(column)),
        };
        if column >= width {
            return None;
        }
        Some(
            source_column
                .and_then(|index| cells.get(index).cloned())
                .unwrap_or_else(|| Value::Undecodable("not fetched".into())),
        )
    }

    #[cfg(test)]
    fn requested(&self) -> usize {
        let slots = self.imp().slots.borrow();
        slots.iter().filter(|slot| matches!(slot, Slot::Cached { .. })).count()
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
#[path = "row_store_tests.rs"]
mod tests;
