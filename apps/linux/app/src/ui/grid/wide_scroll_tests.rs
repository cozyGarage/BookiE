use std::sync::Arc;
use std::time::{Duration, Instant};

use bookie_core::{ColumnInfo, QueryResult, Value};
use gtk4::prelude::*;

use crate::services::database_service::DatabaseService;

use super::{TabGridContext, build_column_view, wrap_grid_scrolled};

const COLUMNS: usize = 40;
const VIEW_WIDTH: i32 = 360;

fn settle() {
    let context = gtk4::glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_millis(250);
    while Instant::now() < deadline {
        while context.iteration(false) {}
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn column(name: String) -> ColumnInfo {
    ColumnInfo {
        name,
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

fn wide_grid() -> (gtk4::ColumnView, gtk4::ScrolledWindow, gtk4::Window) {
    let columns: Vec<ColumnInfo> = (0..COLUMNS).map(|n| column(format!("c{n}"))).collect();
    let rows: Vec<Vec<Value>> = (0..3)
        .map(|row| (0..COLUMNS).map(|n| Value::Text(format!("c{n}-r{row}"))).collect())
        .collect();
    let result = Arc::new(QueryResult {
        columns: columns.clone(),
        rows: rows.into(),
        truncated: false,
    });
    let (view, _) = build_column_view(
        &result,
        &columns,
        "wide",
        None,
        crate::ui::grid::GridSenders {
            edit: None,
            menu: None,
            sort: None,
        },
        crate::ui::grid::GridEnv {
            connection_id: None,
            tab_ctx: TabGridContext::default(),
            column_widths: None,
            highlight_settings: None,
            database: Arc::new(DatabaseService::new()),
        },
    );
    let scrolled = wrap_grid_scrolled(&view);
    let window = gtk4::Window::builder()
        .default_width(VIEW_WIDTH)
        .default_height(240)
        .child(&scrolled)
        .build();
    window.present();
    settle();
    (view, scrolled, window)
}

fn cell_label(root: &impl IsA<gtk4::Widget>, text: &str) -> Option<gtk4::Label> {
    let mut stack = vec![root.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = stack.pop() {
        if let Ok(label) = widget.clone().downcast::<gtk4::Label>()
            && label.text() == text
            && label.is_mapped()
        {
            return Some(label);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            stack.push(current);
            child = stack.last().and_then(|w| w.next_sibling());
        }
    }
    None
}

fn is_inside_viewport(label: &gtk4::Label, scrolled: &gtk4::ScrolledWindow) -> bool {
    let Some(bounds) = label.compute_bounds(scrolled) else {
        return false;
    };
    bounds.x() >= -1.0 && bounds.x() + bounds.width() <= scrolled.width() as f32 + 1.0
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_last_column_of_a_wide_result_is_reachable_by_scrolling() {
    gtk4::init().unwrap();
    let (view, scrolled, window) = wide_grid();
    let adjustment = scrolled.hadjustment();
    assert!(
        adjustment.upper() > adjustment.page_size() * 2.0,
        "the grid must overflow"
    );
    let before = cell_label(&view, &format!("c{}-r0", COLUMNS - 1)).expect("the last column is built");
    assert!(
        !is_inside_viewport(&before, &scrolled),
        "the last column starts outside the viewport"
    );

    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    settle();
    let last = cell_label(&view, &format!("c{}-r0", COLUMNS - 1)).expect("the last column is shown at the far end");
    assert!(is_inside_viewport(&last, &scrolled), "the last column is clipped");
    window.close();
    settle();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn keyboard_focus_moving_right_brings_every_column_into_view_to_the_last() {
    gtk4::init().unwrap();
    let (view, scrolled, window) = wide_grid();
    let adjustment = scrolled.hadjustment();
    view.grab_focus();
    settle();
    for _ in 0..(COLUMNS * 2) {
        view.child_focus(gtk4::DirectionType::Right);
        settle();
    }
    assert!(adjustment.value() > 0.0, "focus moving right never scrolled the grid");
    let end = adjustment.upper() - adjustment.page_size();
    assert!(
        end - adjustment.value() < 2.0,
        "keyboard focus stopped at {} of {end}",
        adjustment.value()
    );
    window.close();
    settle();
}
