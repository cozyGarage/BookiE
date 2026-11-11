use std::sync::Arc;

use gtk4::{self as gtk, prelude::*};

use bookie_core::ColumnInfo;

pub(super) fn install(view: &gtk::ColumnView, columns: &Arc<[ColumnInfo]>) {
    let weak_view = view.downgrade();
    let map_columns = columns.clone();
    view.connect_map(move |_| {
        if let Some(view) = weak_view.upgrade() {
            align_headers(&view, &map_columns);
        }
    });
    let weak_view = view.downgrade();
    let reorder_columns = columns.clone();
    view.columns().connect_items_changed(move |_, _, _, _| {
        let weak_view = weak_view.clone();
        let columns = reorder_columns.clone();
        gtk::glib::idle_add_local_once(move || {
            if let Some(view) = weak_view.upgrade().filter(|view| view.is_mapped()) {
                align_headers(&view, &columns);
            }
        });
    });
}

fn align_headers(view: &gtk::ColumnView, columns: &[ColumnInfo]) {
    let Some(header) = header_widget(view) else {
        return;
    };
    let mut styles = header_styles(view, columns);
    let mut child = header.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        let Some(label) = find_label(&widget) else {
            continue;
        };
        let text = label.text();
        let Some(index) = styles.iter().position(|(title, _)| title == text.as_str()) else {
            continue;
        };
        let (_, right_aligned) = styles.remove(index);
        if right_aligned {
            move_sort_indicator_before_title(&label);
        }
        label.set_hexpand(true);
        label.set_margin_start(8);
        label.set_margin_end(8);
        label.set_xalign(if right_aligned { 1.0 } else { 0.0 });
        label.set_justify(if right_aligned {
            gtk::Justification::Right
        } else {
            gtk::Justification::Left
        });
    }
}

fn move_sort_indicator_before_title(label: &gtk::Label) {
    let Some(parent) = label.parent() else {
        return;
    };
    let Some(container) = parent.clone().downcast::<gtk::Box>().ok() else {
        return;
    };
    let mut child = parent.first_child();
    while let Some(widget) = child {
        if widget.css_name() == "sort-indicator" {
            container.reorder_child_after(&widget, None::<&gtk::Widget>);
            return;
        }
        child = widget.next_sibling();
    }
}

fn header_styles(view: &gtk::ColumnView, columns: &[ColumnInfo]) -> Vec<(String, bool)> {
    let model = view.columns();
    (0..model.n_items())
        .filter_map(|position| model.item(position)?.downcast::<gtk::ColumnViewColumn>().ok())
        .filter_map(|column| {
            let title = column.title()?.to_string();
            let right_aligned = match column.id().as_deref() {
                Some(super::row_number_column::COLUMN_ID) => true,
                Some(id) => id
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| columns.get(index))
                    .is_some_and(super::column::right_aligned_column),
                None => false,
            };
            Some((title, right_aligned))
        })
        .collect()
}

fn header_widget(view: &gtk::ColumnView) -> Option<gtk::Widget> {
    let mut child = view.first_child();
    while let Some(widget) = child {
        if widget.css_name() == "header" {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

fn find_label(root: &gtk::Widget) -> Option<gtk::Label> {
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
            return Some(label);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            stack.push(current.clone());
            child = current.next_sibling();
        }
    }
    None
}
