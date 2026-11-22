use std::sync::Arc;
use std::time::Instant;

use bookie_gpui::model::{Cell, ColumnMeta, Diff, FindResult, Predicate};
use bookie_gpui::source::ResultSource;
use gpui::TestAppContext;
use gpui_kit::component::scroll::ScrollbarHandle;
use gpui_kit::component::table::{ColumnFixed, TableDelegate};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use tokio_util::sync::CancellationToken;

use crate::{GridView, text_input};

struct WideSource(Vec<ColumnMeta>);

impl ResultSource for WideSource {
    fn columns(&self) -> &[ColumnMeta] {
        &self.0
    }

    fn total(&self) -> u64 {
        5_000
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        self.0
            .iter()
            .map(|column| Cell::Text(format!("row-{index}-{}", column.name)))
            .collect()
    }

    fn find(&self, _: &str, _: usize, _: &CancellationToken) -> Result<FindResult, &'static str> {
        Err("not used by this test source")
    }

    fn filter(&self, _: &Predicate, _: &CancellationToken) -> Result<Vec<u32>, &'static str> {
        Err("not used by this test source")
    }

    fn compare(&self, _: bool, _: &CancellationToken) -> Result<Diff, &'static str> {
        Err("not used by this test source")
    }
}

#[gpui::test]
fn hidden_middle_column_keeps_visible_cells_bound_to_their_source_columns(cx: &mut TestAppContext) {
    let source = Arc::new(WideSource(
        ["id", "name", "secret"]
            .into_iter()
            .map(|name| ColumnMeta {
                name: name.into(),
                width: 120.0,
                numeric: false,
                data_type: "text".into(),
                primary_key: false,
            })
            .collect(),
    ));
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.simulate_resize(gpui::size(gpui::px(900.0), gpui::px(760.0)));
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        let names = view.source.columns().iter().map(|column| column.name.clone()).collect::<Vec<_>>();
        let hidden = std::collections::HashSet::from(["name".to_string()]);
        view.table.update(cx, |table, cx| {
            let delegate = table.delegate_mut();
            delegate.set_hidden_columns(&names, hidden);
            table.refresh(cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let label = view.update(cx, |view, cx| view.table.update(cx, |table, _| table.delegate_mut().label_of(0, 2).0));
    view.read_with(cx, |view, app| {
        let table = view.table.read(app);
        let delegate = table.delegate();
        assert_eq!(delegate.columns_count(app), 2);
        assert_eq!(delegate.column(1, app).name.as_ref(), "secret");
        assert_eq!(delegate.source_column(1), Some(2));
    });
    assert_eq!(label, "row-0-secret");
}

#[gpui::test]
fn pinned_columns_stay_visible_after_scroll_and_tab_reopen(cx: &mut TestAppContext) {
    let source = Arc::new(WideSource(
        (0..60)
            .map(|index| ColumnMeta {
                name: format!("column_{index}"),
                width: 150.0,
                numeric: false,
                data_type: "text".into(),
                primary_key: index == 0,
            })
            .collect(),
    ));
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.simulate_resize(gpui::size(gpui::px(900.0), gpui::px(760.0)));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let menu_bounds = cx.update(|window, _| window.find_by_label("Actions for column_0").bounds());
    cx.simulate_click(menu_bounds.center(), Default::default());
    cx.run_until_parked();
    cx.update(|window, _| {
        assert!(window.try_find_by_label("Copy column name").is_some());
        assert!(window.try_find_by_label("Sort ascending").is_none());
    });
    let copy_bounds = cx.update(|window, _| window.find_by_label("Copy column name").bounds());
    cx.simulate_click(copy_bounds.center(), Default::default());
    cx.run_until_parked();

    cx.update(|window, cx| window.render_frame(cx));
    let header = cx.update(|window, _| {
        window
            .try_find(("col-header", 1usize))
            .expect("second column header is rendered")
            .bounds()
    });
    cx.simulate_click(
        gpui::point(header.origin.x + header.size.width - gpui::px(12.0), header.center().y),
        Default::default(),
    );
    cx.run_until_parked();

    view.update(cx, |view, cx| {
        view.table.update(cx, |table, cx| table.scroll_to_col(59, cx))
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let pinned = window
            .try_find(("col-header", 0usize))
            .expect("first pinned header remains rendered");
        let last = window
            .try_find(("col-header", 59usize))
            .expect("last column is rendered after scrolling");
        assert!(pinned.visible(), "pinned header is outside the viewport");
        assert!(last.visible(), "scrolled-to header is outside the viewport");
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let table = view.catalog.grid_table.clone();
            view.tabs.close(view.tabs.active);
            view.tabs.open_table(&table);
            view.sync_table(900.0, window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let table = view.table.read(cx);
        assert_eq!(table.delegate().column(0, cx).fixed, Some(ColumnFixed::Left));
        assert_eq!(table.delegate().column(1, cx).fixed, Some(ColumnFixed::Left));
        assert_eq!(table.delegate().column(2, cx).fixed, None);
    });
}

#[gpui::test]
fn result_grid_scrollbars_reserve_space_and_keep_the_last_cell_visible(cx: &mut TestAppContext) {
    let source = Arc::new(WideSource(
        (0..60)
            .map(|index| ColumnMeta {
                name: format!("column_{index}"),
                width: 150.0,
                numeric: false,
                data_type: "text".into(),
                primary_key: false,
            })
            .collect(),
    ));
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.simulate_resize(gpui::size(gpui::px(900.0), gpui::px(760.0)));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));

    let viewport = cx
        .debug_bounds("grid-table-viewport")
        .expect("table viewport is rendered");
    let vertical_gutter = cx
        .debug_bounds("grid-vertical-scrollbar-gutter")
        .expect("vertical scrollbar gutter is rendered");
    let horizontal_gutter = cx
        .debug_bounds("grid-horizontal-scrollbar-gutter")
        .expect("horizontal scrollbar gutter is rendered");
    assert!(viewport.origin.x + viewport.size.width <= vertical_gutter.origin.x);
    assert!(viewport.origin.y + viewport.size.height <= horizontal_gutter.origin.y);

    view.update(cx, |view, cx| {
        view.table.update(cx, |table, cx| table.set_selected_cell(4_999, 59, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    view.read_with(cx, |view, app| {
        let table = view.table.read(app);
        assert_eq!(table.selected_cell(), Some((4_999, 59)));
        let horizontal_viewport = table.horizontal_scroll_handle.viewport_bounds();
        let vertical_viewport = table.vertical_scroll_handle.0.borrow().base_handle.bounds();
        assert!(horizontal_viewport.origin.x + horizontal_viewport.size.width <= vertical_gutter.origin.x);
        assert!(vertical_viewport.origin.y + vertical_viewport.size.height <= horizontal_gutter.origin.y);
    });
    cx.update(|window, _| {
        let last_cell = window.find("table-cell-4999:59").bounds();
        assert!(last_cell.origin.x >= viewport.origin.x);
        assert!(last_cell.origin.y >= viewport.origin.y);
        assert!(last_cell.origin.x + last_cell.size.width <= vertical_gutter.origin.x);
        assert!(last_cell.origin.y + last_cell.size.height <= horizontal_gutter.origin.y);
        let last_header = window.find(("col-header", 59usize)).bounds();
        assert!(last_header.origin.x + last_header.size.width <= vertical_gutter.origin.x);
        assert!(last_header.origin.x >= viewport.origin.x);
    });
}
