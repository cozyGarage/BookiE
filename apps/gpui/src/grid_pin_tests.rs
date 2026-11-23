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

struct BinarySource;

impl ResultSource for BinarySource {
    fn columns(&self) -> &[ColumnMeta] {
        static COLUMNS: std::sync::OnceLock<Vec<ColumnMeta>> = std::sync::OnceLock::new();
        COLUMNS.get_or_init(|| {
            vec![
                ColumnMeta {
                    name: "name".into(),
                    width: 120.0,
                    numeric: false,
                    data_type: "text".into(),
                    primary_key: false,
                },
                ColumnMeta {
                    name: "payload".into(),
                    width: 160.0,
                    numeric: false,
                    data_type: "bytea".into(),
                    primary_key: false,
                },
            ]
        })
    }

    fn editable(&self) -> bool {
        true
    }

    fn total(&self) -> u64 {
        1
    }

    fn row(&self, _: u64) -> Vec<Cell> {
        vec![Cell::Text("blob".into()), Cell::Bytes(vec![0, 255, 65])]
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
        let column_count = view.source.columns().len();
        let hidden = std::collections::HashSet::from([1]);
        view.table.update(cx, |table, cx| {
            let delegate = table.delegate_mut();
            delegate.set_hidden_columns(column_count, hidden);
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
fn binary_display_mode_controls_preview_detail_and_edit_without_changing_value_type(cx: &mut TestAppContext) {
    cx.update(text_input::bind_keys);
    let source = Arc::new(BinarySource);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));

    let menu = cx.update(|window, _| window.find_by_label("Actions for payload").bounds());
    cx.simulate_click(menu.center(), Default::default());
    cx.run_until_parked();
    let base64 = cx.update(|window, _| window.find_by_label("Display as Base64").bounds());
    cx.simulate_click(base64.center(), Default::default());
    cx.run_until_parked();

    view.update(cx, |view, cx| {
        assert_eq!(view.table.update(cx, |table, _| table.delegate_mut().label_of(0, 1).0), "AP9B");
        view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
        assert_eq!(
            view.detail_target().map(|(_, detail)| detail),
            Some(bookie_gpui::detail::Detail::Text("AP9B".into()))
        );
    });

    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
    view.read_with(cx, |view, cx| assert_eq!(view.cell_edit.editor.read(cx).content(), "AP9B"));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("bad=");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty());
        assert!(view.status.contains("not valid Base64 bytes"));
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("AP9B");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(0, 1)), Some(&Cell::Bytes(vec![0, 255, 65])));
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.source = Arc::new(BinarySource);
            view.table_source = view.source.clone();
            view.sync_table(900.0, window, cx);
        })
    });
    view.update(cx, |view, cx| {
        assert_eq!(view.table.update(cx, |table, _| table.delegate_mut().label_of(0, 1).0), "AP9B");
    });
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
    let (view, cx) = cx.add_window_view(|window, cx| {
        GridView::with_source(source, Instant::now(), window, cx)
    });
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
fn narrow_column_headers_keep_pin_actions_in_the_menu(cx: &mut TestAppContext) {
    let source = Arc::new(WideSource(
        (0..40)
            .map(|index| ColumnMeta {
                name: format!("c{index:02}"),
                width: 56.0,
                numeric: false,
                data_type: "text".into(),
                primary_key: false,
            })
            .collect(),
    ));
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| {
        GridView::with_source(source, Instant::now(), window, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1_896.0), gpui::px(1_150.0)));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));

    assert!(
        cx.update(|window, _| window.try_find_by_label("Pin through c00").is_none()),
        "a narrow header should reserve its width for the column name"
    );
    let name_width = cx
        .debug_bounds("column-header-name-0")
        .expect("column name is rendered")
        .size
        .width;
    assert!(
        name_width >= gpui::px(20.0),
        "a short column name should retain enough width to remain recognizable: {name_width:?}"
    );
    let menu_bounds = cx.update(|window, _| window.find_by_label("Actions for c00").bounds());
    cx.simulate_click(menu_bounds.center(), Default::default());
    cx.run_until_parked();
    assert!(
        cx.update(|window, _| window.try_find_by_label("Pin through c00").is_some()),
        "the column menu should retain the pin action when the header button is hidden"
    );
    let pin_bounds = cx.update(|window, _| window.find_by_label("Pin through c00").bounds());
    cx.simulate_click(pin_bounds.center(), Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, app| {
        assert_eq!(
            view.table.read(app).delegate().column(0, app).fixed,
            Some(ColumnFixed::Left)
        );
    });

    cx.simulate_resize(gpui::size(gpui::px(800.0), gpui::px(850.0)));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let narrow_menu_bounds = cx.update(|window, _| window.find_by_label("Actions for c01").bounds());
    cx.simulate_click(narrow_menu_bounds.center(), Default::default());
    cx.run_until_parked();
    assert!(
        cx.update(|window, _| window.try_find_by_label("Pin through c01").is_some()),
        "the pin action should stay reachable after the result window narrows"
    );
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
