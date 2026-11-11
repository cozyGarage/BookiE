use std::cell::RefCell;
use std::rc::Rc;

use gtk4::{glib, prelude::*};

pub(super) struct CellContext {
    pub(super) widget: gtk4::Widget,
    pub(super) col_index: usize,
    pub(super) column_name: String,
    pub(super) draft_target: Option<(u32, u64)>,
    pub(super) row_position: Option<u32>,
    pub(super) row_key: Vec<bookie_core::Value>,
    pub(super) row: Option<crate::ui::row_object::RowObject>,
}

struct WeakCellContext {
    widget: glib::WeakRef<gtk4::Widget>,
    col_index: usize,
    column_name: String,
    draft_target: Option<(u32, u64)>,
    row_position: Option<u32>,
    row_key: Vec<bookie_core::Value>,
    row: Option<glib::WeakRef<crate::ui::row_object::RowObject>>,
}

#[derive(Clone, Default)]
pub(super) struct CellContextStore(Rc<RefCell<Option<WeakCellContext>>>);

impl CellContextStore {
    pub(super) fn set(
        &self,
        widget: &gtk4::Widget,
        col_index: usize,
        column_name: String,
        draft_target: Option<(u32, u64)>,
        row: Option<&crate::ui::row_object::RowObject>,
    ) {
        self.0.replace(Some(WeakCellContext {
            widget: widget.downgrade(),
            col_index,
            column_name,
            draft_target,
            row: row.map(|row| row.downgrade()),
            row_position: super::display::POSITION_SLOT.get(widget),
            row_key: super::display::ROW_KEY_SLOT
                .cloned(widget)
                .map_or_else(Vec::new, |key| key.to_vec()),
        }));
    }

    pub(super) fn snapshot(&self) -> Option<CellContext> {
        let state = self.0.borrow();
        let state = state.as_ref()?;
        Some(CellContext {
            widget: state.widget.upgrade()?,
            col_index: state.col_index,
            column_name: state.column_name.clone(),
            draft_target: state.draft_target,
            row_position: state.row_position,
            row_key: state.row_key.clone(),
            row: state.row.as_ref().and_then(glib::WeakRef::upgrade),
        })
    }
}
