use std::sync::Arc;
use std::time::{Duration, Instant};

use bookie_core::{ColumnInfo, QueryResult, Value};
use gtk4::prelude::*;

use crate::services::column_widths::ColumnWidthStore;
use crate::services::database_service::DatabaseService;

use super::{TabGridContext, build_column_view, wrap_grid_scrolled};

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

fn settle() {
    let context = gtk4::glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_millis(150);
    while Instant::now() < deadline {
        while context.iteration(false) {}
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn cell(widget: &gtk4::Widget, text: &str) -> Option<gtk4::Widget> {
    if widget
        .clone()
        .downcast::<gtk4::Label>()
        .is_ok_and(|label| label.text() == text)
    {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = cell(&current, text) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn verify_fixed_width(saved: bool, persistent: bool) {
    gtk4::init().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let store = ColumnWidthStore::load_in(directory.path());
    let id = uuid::Uuid::new_v4();
    if saved {
        store.save(id, "items", "first", 140).unwrap();
    }
    let columns = vec![column("first"), column("second")];
    let result = Arc::new(QueryResult {
        columns: columns.clone(),
        rows: vec![vec![
            Value::Text("first-value".into()),
            Value::Text("second-value".into()),
        ]]
        .into(),
        truncated: false,
    });
    let (view, _) = build_column_view(
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
            connection_id: persistent.then_some(id),
            tab_ctx: TabGridContext::default(),
            column_widths: persistent.then_some(store.clone()),
            highlight_settings: None,
            database: Arc::new(DatabaseService::new()),
        },
    );
    let window = gtk4::Window::builder()
        .default_width(500)
        .default_height(300)
        .child(&crate::ui::grid::wrap_grid_scrolled(&view))
        .build();
    window.present();
    settle();
    let first = (0..view.columns().n_items())
        .find_map(|position| {
            let column = view
                .columns()
                .item(position)?
                .downcast::<gtk4::ColumnViewColumn>()
                .ok()?;
            (column.id().as_deref() == Some("0")).then_some(column)
        })
        .expect("the first data column");
    if !saved {
        first.set_fixed_width(140);
        settle();
    }
    let fixed_cell = cell(view.upcast_ref(), "first-value").unwrap();
    let expanding_cell = cell(view.upcast_ref(), "second-value").unwrap();
    let before = (fixed_cell.width(), expanding_cell.width());
    window.set_size_request(800, 300);
    settle();
    assert!(
        view.width() >= 750,
        "the view did not resize: view={} window={} default={}",
        view.width(),
        window.width(),
        window.default_width()
    );
    assert_eq!(fixed_cell.width(), before.0, "the manual width changed with the window");
    assert!(
        expanding_cell.width() > before.1,
        "the unsized column must still fill available space"
    );
    if persistent {
        assert_eq!(store.load(id, "items", "first"), Some(140));
    }
    window.close();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn saved_grid_width_stays_fixed_when_the_window_grows() {
    verify_fixed_width(true, true);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn resized_grid_width_stays_fixed_when_the_window_grows() {
    verify_fixed_width(false, true);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn unsaved_result_grid_width_stays_fixed_when_the_window_grows() {
    verify_fixed_width(false, false);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn column_header_text_aligns_with_numeric_and_text_cells() {
    gtk4::init().unwrap();
    let numeric = ColumnInfo {
        data_type: "INTEGER".into(),
        ..column("value")
    };
    let text = column("name");
    let columns = vec![numeric, text];
    let result = Arc::new(QueryResult {
        columns: columns.clone(),
        rows: vec![vec![Value::Int(13), Value::Text("Ada".into())]].into(),
        truncated: false,
    });
    let (view, _) = build_column_view(
        &result,
        &columns,
        "items",
        Some((0, true)),
        crate::ui::grid::GridSenders {
            edit: None,
            menu: None,
            sort: None,
        },
        crate::ui::grid::GridEnv {
            connection_id: None,
            tab_ctx: TabGridContext {
                local_result_sort: true,
                sortable_result_columns: [0, 1].into_iter().collect(),
                ..Default::default()
            },
            column_widths: None,
            highlight_settings: None,
            database: Arc::new(DatabaseService::new()),
        },
    );
    let window = gtk4::Window::builder()
        .default_width(500)
        .default_height(300)
        .child(&wrap_grid_scrolled(&view))
        .build();
    window.present();
    settle();
    assert_grid_header_alignment(&view);
    let numeric_column = (0..view.columns().n_items())
        .filter_map(|position| view.columns().item(position)?.downcast::<gtk4::ColumnViewColumn>().ok())
        .find(|column| column.id().as_deref() == Some("0"))
        .unwrap();
    view.remove_column(&numeric_column);
    view.insert_column(2, &numeric_column);
    settle();
    assert_grid_header_alignment(&view);
    window.close();
    settle();
}

fn assert_grid_header_alignment(view: &gtk4::ColumnView) {
    let root = view.upcast_ref();
    assert_aligned_text(
        &cell(root, "name\nTEXT").unwrap(),
        &cell(root, "Ada").unwrap(),
        root,
        false,
    );
    assert_aligned_text(
        &cell(root, "value\nINTEGER").unwrap(),
        &cell(root, "13").unwrap(),
        root,
        true,
    );
}

fn assert_aligned_text(header: &gtk4::Widget, cell: &gtk4::Widget, root: &gtk4::Widget, right: bool) {
    let header_bounds = header.compute_bounds(root).unwrap();
    let cell_bounds = cell.compute_bounds(root).unwrap();
    let header_label = header.clone().downcast::<gtk4::Label>().unwrap();
    let cell_label = cell.clone().downcast::<gtk4::Label>().unwrap();
    let header_ink = header_label.layout().pixel_extents().0;
    let cell_ink = cell_label.layout().pixel_extents().0;
    let header_offset = header_label.layout_offsets().0;
    let cell_offset = cell_label.layout_offsets().0;
    let header_edge = if right {
        header_bounds.x() + header_offset as f32 + header_ink.x() as f32 + header_ink.width() as f32
    } else {
        header_bounds.x() + header_offset as f32 + header_ink.x() as f32
    };
    let cell_edge = if right {
        cell_bounds.x() + cell_offset as f32 + cell_ink.x() as f32 + cell_ink.width() as f32
    } else {
        cell_bounds.x() + cell_offset as f32 + cell_ink.x() as f32
    };
    assert!(
        (header_edge - cell_edge).abs() <= 2.0,
        "header={header_edge}, cell={cell_edge}, header_bounds={header_bounds:?}, cell_bounds={cell_bounds:?}, header_xalign={}, cell_xalign={}",
        header_label.xalign(),
        cell_label.xalign()
    );
}
