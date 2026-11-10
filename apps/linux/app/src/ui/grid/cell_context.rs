use std::cell::RefCell;
use std::rc::Rc;

use gtk4::{glib, prelude::*};

pub(super) struct CellContext {
    pub(super) widget: gtk4::Widget,
    pub(super) col_index: usize,
    pub(super) column_name: String,
    pub(super) draft_target: Option<(u32, u64)>,
}

struct WeakCellContext {
    widget: glib::WeakRef<gtk4::Widget>,
    col_index: usize,
    column_name: String,
    draft_target: Option<(u32, u64)>,
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
    ) {
        self.0.replace(Some(WeakCellContext {
            widget: widget.downgrade(),
            col_index,
            column_name,
            draft_target,
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
        })
    }
}
