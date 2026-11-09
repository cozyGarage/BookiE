use std::sync::Arc;
use std::time::{Duration, Instant};

use bookie_core::{ColumnInfo, QueryResult, Value};
use gtk4::prelude::*;

use crate::services::database_service::DatabaseService;

use super::{TabGridContext, build_column_view, wrap_grid_scrolled};

fn settle() {
    let context = gtk4::glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_millis(200);
    while Instant::now() < deadline {
        while context.iteration(false) {}
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "TEXT".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn wide_grid(base: u64) -> (gtk4::ColumnView, gtk4::MultiSelection, gtk4::Window) {
    let columns = vec![column("first"), column("second"), column("third"), column("fourth")];
    let result = Arc::new(QueryResult {
        columns: columns.clone(),
        rows: vec![
            vec![
                Value::Text("a1".into()),
                Value::Text("a2".into()),
                Value::Text("a3".into()),
                Value::Text("a4-wide-value".into()),
            ],
            vec![
                Value::Text("b1".into()),
                Value::Text("b2".into()),
                Value::Text("b3".into()),
                Value::Text("b4-wide-value".into()),
            ],
        ]
        .into(),
        truncated: false,
    });
    let (view, selection) = build_column_view(
        &result,
        &columns,
        "items",
        None,
        crate::ui::grid::GridSenders {
            edit: None,
            menu: None,
            sort: None,
        },
        crate::ui::grid::GridEnv {
            connection_id: None,
            tab_ctx: TabGridContext {
                row_number_base: base,
                ..Default::default()
            },
            column_widths: None,
            highlight_settings: None,
            database: Arc::new(DatabaseService::new()),
        },
    );
    for position in 0..view.columns().n_items() {
        if let Some(column) = view
            .columns()
            .item(position)
            .and_then(|item| item.downcast::<gtk4::ColumnViewColumn>().ok())
            && column.id().as_deref() != Some(super::row_number_column::COLUMN_ID)
        {
            column.set_fixed_width(220);
        }
    }
    let scrolled = wrap_grid_scrolled(&view);
    let window = gtk4::Window::builder()
        .default_width(360)
        .default_height(240)
        .child(&scrolled)
        .build();
    window.present();
    settle();
    (view, selection, window)
}

fn number_labels(view: &gtk4::ColumnView) -> Vec<String> {
    let mut labels = Vec::new();
    let mut stack = vec![view.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = stack.pop() {
        if let Ok(label) = widget.clone().downcast::<gtk4::Label>()
            && label
                .parent()
                .is_some_and(|parent| parent.has_css_class("row-number-cell"))
        {
            labels.push(label.text().to_string());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            stack.push(current.clone());
            child = current.next_sibling();
        }
    }
    labels.sort();
    labels.dedup();
    labels
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn result_grids_number_rows_from_one_and_keep_the_gutter_after_horizontal_scroll() {
    gtk4::init().unwrap();
    let (view, selection, window) = wide_grid(0);
    let first = view
        .columns()
        .item(0)
        .and_then(|item| item.downcast::<gtk4::ColumnViewColumn>().ok())
        .expect("the leading column");
    assert_eq!(first.id().as_deref(), Some(super::row_number_column::COLUMN_ID));
    assert_eq!(number_labels(&view), vec!["1".to_owned(), "2".to_owned()]);

    let scrolled = view
        .ancestor(gtk4::ScrolledWindow::static_type())
        .and_then(|widget| widget.downcast::<gtk4::ScrolledWindow>().ok())
        .expect("the grid scroller");
    let adjustment = scrolled.hadjustment();
    assert!(
        adjustment.upper() > adjustment.page_size(),
        "the grid must overflow horizontally"
    );
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    settle();
    assert!(adjustment.value() > 0.0);

    let backing = super::pinned_numbers::gutter_backing(&view).expect("the sticky gutter backing");
    assert!(backing.is_visible(), "horizontal scroll must keep the gutter cover");
    assert_eq!(number_labels(&view), vec!["1".to_owned(), "2".to_owned()]);

    selection.select_item(1, true);
    assert!(selection.is_selected(1));
    assert!(!selection.is_selected(0));
    window.close();
    settle();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn browse_offset_shifts_the_leading_row_numbers() {
    gtk4::init().unwrap();
    let (view, _, window) = wide_grid(100);
    assert_eq!(number_labels(&view), vec!["101".to_owned(), "102".to_owned()]);
    window.close();
    settle();
}
