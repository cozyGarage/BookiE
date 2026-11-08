use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{self as gtk, glib, pango};

use crate::ui::row_object::RowObject;

pub(super) const COLUMN_ID: &str = "row-numbers";

const CHROME_PX: i32 = 16;

type Watches = Rc<RefCell<HashMap<glib::Object, Vec<(glib::WeakRef<glib::Object>, glib::SignalHandlerId)>>>>;

pub(super) fn build(base: Rc<Cell<u64>>) -> gtk::ColumnViewColumn {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(cell) = item.downcast_ref::<gtk::ColumnViewCell>() else {
            return;
        };
        cell.set_property("focusable", false);
        let number = gtk::Label::builder()
            .xalign(1.0)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .css_classes(["numeric", "monospace", "dimmed"])
            .build();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        content.add_css_class("row-number-cell");
        content.append(&number);
        cell.set_child(Some(&content));
    });
    let watches: Watches = Rc::default();
    {
        let watches = watches.clone();
        let base = base.clone();
        factory.connect_bind(move |_, item| {
            let Some(cell) = item.downcast_ref::<gtk::ColumnViewCell>() else {
                return;
            };
            bind_cell(cell, &base, &watches);
        });
    }
    factory.connect_unbind(move |_, item| {
        unwatch(&watches, item.upcast_ref());
    });
    let column = gtk::ColumnViewColumn::builder()
        .title("#")
        .factory(&factory)
        .resizable(false)
        .build();
    column.set_id(Some(COLUMN_ID));
    column
}

pub(super) fn fit(view: &gtk::ColumnView, base: u64) {
    let Some(column) = row_numbers_column(view) else {
        return;
    };
    let rows = view.model().map_or(0, |model| u64::from(model.n_items()));
    let largest = base.saturating_add(rows);
    let digits = largest.max(1).ilog10() as i32 + 1;
    let context = view.create_pango_context();
    let metrics = context.metrics(None, None);
    let digit_px = pango::units_to_double(metrics.approximate_digit_width());
    column.set_fixed_width((f64::from(digits.max(2)) * digit_px).ceil() as i32 + CHROME_PX);
}

pub(super) fn row_numbers_column(view: &gtk::ColumnView) -> Option<gtk::ColumnViewColumn> {
    let columns = view.columns();
    (0..columns.n_items()).find_map(|position| {
        let column = columns.item(position)?.downcast::<gtk::ColumnViewColumn>().ok()?;
        (column.id().as_deref() == Some(COLUMN_ID)).then_some(column)
    })
}

pub(super) fn keep_first(view: &gtk::ColumnView) {
    let Some(column) = row_numbers_column(view) else {
        return;
    };
    let Some(first) = view
        .columns()
        .item(0)
        .and_then(|item| item.downcast::<gtk::ColumnViewColumn>().ok())
    else {
        return;
    };
    if first != column {
        view.insert_column(0, &column);
    }
}

fn bind_cell(cell: &gtk::ColumnViewCell, base: &Rc<Cell<u64>>, watches: &Watches) {
    let show = {
        let cell = cell.downgrade();
        let base = base.clone();
        move || {
            if let Some(cell) = cell.upgrade() {
                show_cell(&cell, base.get());
            }
        }
    };
    show();
    let on_position = {
        let show = show.clone();
        cell.connect_notify_local(Some("position"), move |_, _| show())
    };
    watches.borrow_mut().insert(
        cell.clone().upcast(),
        vec![(cell.upcast_ref::<glib::Object>().downgrade(), on_position)],
    );
}

fn show_cell(cell: &gtk::ColumnViewCell, base: u64) {
    let Some(content) = cell.child() else {
        return;
    };
    let Some(number) = content.first_child().and_downcast::<gtk::Label>() else {
        return;
    };
    number.set_text(row_label(cell, base).as_deref().unwrap_or(""));
}

fn row_label(cell: &gtk::ColumnViewCell, base: u64) -> Option<String> {
    let position = cell.position();
    if position == gtk::INVALID_LIST_POSITION {
        return None;
    }
    let row = cell.item().and_downcast::<RowObject>();
    let draft = row.as_ref().is_some_and(|row| row.draft_id().is_some());
    let source = row.as_ref().and_then(|row| row.source_index());
    number_for(base, position, source, draft).map(|number| number.to_string())
}

pub(super) fn number_for(base: u64, position: u32, source_index: Option<usize>, draft: bool) -> Option<u64> {
    if draft {
        return None;
    }
    Some(match source_index {
        Some(source) => base.saturating_add(source as u64).saturating_add(1),
        None => base.saturating_add(u64::from(position)).saturating_add(1),
    })
}

fn unwatch(watches: &Watches, item: &glib::Object) {
    if let Some(handlers) = watches.borrow_mut().remove(item) {
        for (object, handler) in handlers {
            if let Some(object) = object.upgrade() {
                object.disconnect(handler);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_offsets_and_drafts_choose_the_displayed_number() {
        assert_eq!(number_for(0, 0, Some(0), false), Some(1));
        assert_eq!(number_for(0, 1, Some(1), false), Some(2));
        assert_eq!(number_for(100, 0, Some(0), false), Some(101));
        assert_eq!(number_for(0, 3, None, false), Some(4));
        assert_eq!(number_for(100, 0, None, true), None);
    }
}
