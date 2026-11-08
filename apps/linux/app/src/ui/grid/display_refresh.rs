use gtk4::prelude::*;

use crate::ui::row_object::RowObject;
use crate::ui::row_store::RowStore;

pub fn refresh_date_displays_under(root: &impl IsA<gtk4::Widget>) {
    let mut pending = vec![root.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = pending.pop() {
        if let Ok(view) = widget.clone().downcast::<gtk4::ColumnView>() {
            rebind_column_view(&view);
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            pending.push(next);
        }
    }
}

fn rebind_column_view(view: &gtk4::ColumnView) {
    let Some(selection) = view.model() else {
        return;
    };
    let Some(model) = selection.model() else {
        return;
    };
    let Ok(store) = model.downcast::<RowStore>() else {
        return;
    };
    rebind_row_store(&store);
}

fn rebind_row_store(store: &RowStore) {
    let count = store.n_items();
    if count == 0 {
        return;
    }
    let mut rows = Vec::with_capacity(count as usize);
    for position in 0..count {
        let Some(item) = store.item(position) else {
            return;
        };
        let Ok(row) = item.downcast::<RowObject>() else {
            return;
        };
        rows.push(row.clone_preserving_loading_state());
    }
    if rows.len() as u32 == count {
        store.splice(0, count, &rows);
    }
}
