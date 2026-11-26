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

struct QuerySource {
    inner: WideSource,
    sql: &'static str,
    sql_reads: Arc<std::sync::atomic::AtomicUsize>,
}

struct FitSource {
    columns: Vec<ColumnMeta>,
    row: Vec<Cell>,
}

impl ResultSource for FitSource {
    fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }
    fn total(&self) -> u64 {
        1
    }
    fn editable(&self) -> bool {
        true
    }
    fn row(&self, _: u64) -> Vec<Cell> {
        self.row.clone()
    }
    fn row_ref(&self, _: u64) -> Option<&[Cell]> {
        Some(&self.row)
    }
    fn find(&self, _: &str, _: usize, _: &CancellationToken) -> Result<FindResult, &'static str> {
        Err("unused")
    }
    fn filter(&self, _: &Predicate, _: &CancellationToken) -> Result<Vec<u32>, &'static str> {
        Err("unused")
    }
    fn compare(&self, _: bool, _: &CancellationToken) -> Result<Diff, &'static str> {
        Err("unused")
    }
}

#[gpui::test]
fn multiline_and_large_binary_edits_use_full_value_editor_without_staging_unchanged_values(cx: &mut TestAppContext) {
    let source = Arc::new(FitSource {
        columns: vec![
            ColumnMeta {
                name: "id".into(),
                width: 120.0,
                numeric: true,
                data_type: "integer".into(),
                primary_key: true,
            },
            ColumnMeta {
                name: "payload".into(),
                width: 120.0,
                numeric: false,
                data_type: "bytea".into(),
                primary_key: false,
            },
            ColumnMeta {
                name: "large_payload".into(),
                width: 120.0,
                numeric: false,
                data_type: "bytea".into(),
                primary_key: false,
            },
            ColumnMeta {
                name: "oversized_payload".into(),
                width: 120.0,
                numeric: false,
                data_type: "bytea".into(),
                primary_key: false,
            },
        ],
        row: vec![
            Cell::Int(1),
            Cell::Bytes(b"first\nsecond".to_vec()),
            Cell::Bytes(vec![b'x'; 513]),
            Cell::Bytes(vec![b'y'; 524_288]),
        ],
    });
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_binary_mode(1, crate::binary_mode::BinaryMode::Utf8, cx);
            view.start_edit(0, 1, window, cx);
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let editing = view.cell_edit.active.as_ref().expect("binary cell is being edited");
        assert_eq!(editing.binary_mode, Some(crate::binary_mode::BinaryMode::Utf8));
        assert!(editing.full_value);
        assert_eq!(view.cell_edit.value_editor.read(cx).value().as_ref(), "first\nsecond");
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.apply_active_cell_edit(window, cx)));
    view.read_with(cx, |view, _| assert!(view.cell_edit.edits.is_empty()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_binary_mode(2, crate::binary_mode::BinaryMode::Utf8, cx);
            view.start_edit(0, 2, window, cx);
        });
    });
    view.read_with(cx, |view, cx| {
        assert!(view.cell_edit.active.as_ref().is_some_and(|editing| editing.full_value));
        assert_eq!(view.cell_edit.value_editor.read(cx).value().len(), 513);
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.apply_active_cell_edit(window, cx)));
    view.read_with(cx, |view, _| assert!(view.cell_edit.edits.is_empty()));
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 3, window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.active.is_none());
        assert!(view.status.contains("1 MiB"));
    });
}

#[gpui::test]
fn automatic_primary_key_width_leaves_room_for_the_inline_type_and_controls(cx: &mut TestAppContext) {
    let source = Arc::new(FitSource {
        columns: vec![ColumnMeta {
            name: "id".into(),
            width: 72.0,
            numeric: true,
            data_type: "integer".into(),
            primary_key: true,
        }],
        row: vec![Cell::Int(1)],
    });
    cx.update(text_input::bind_keys);
    let (_, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.run_until_parked();
    let type_bounds = cx.debug_bounds("column-header-type-0").expect("inline type");
    let action_bounds = cx.update(|window, _| window.find_by_label("Actions for id").bounds());
    assert!(
        action_bounds.size.width <= gpui::px(bookie_gpui::theme::size::COMPACT_CONTROL),
        "header actions must use the compact control size"
    );
    let text_width = cx.update(|window, _| {
        let style = window.text_style();
        window
            .text_system()
            .shape_line(
                "INTEGER".into(),
                gpui::px(bookie_gpui::theme::size::TEXT_SMALL),
                &[gpui::TextRun {
                    len: "INTEGER".len(),
                    font: style.font(),
                    color: style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width
    });
    assert!(
        type_bounds.size.width >= text_width,
        "the type must remain readable beside header actions"
    );
}

impl ResultSource for QuerySource {
    fn columns(&self) -> &[ColumnMeta] {
        self.inner.columns()
    }
    fn total(&self) -> u64 {
        self.inner.total()
    }
    fn executed_sql(&self) -> Option<&str> {
        self.sql_reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(self.sql)
    }
    fn row(&self, index: u64) -> Vec<Cell> {
        self.inner.row(index)
    }
    fn find(&self, q: &str, cap: usize, cancel: &CancellationToken) -> Result<FindResult, &'static str> {
        self.inner.find(q, cap, cancel)
    }
    fn filter(&self, predicate: &Predicate, cancel: &CancellationToken) -> Result<Vec<u32>, &'static str> {
        self.inner.filter(predicate, cancel)
    }
    fn compare(&self, keyed: bool, cancel: &CancellationToken) -> Result<Diff, &'static str> {
        self.inner.compare(keyed, cancel)
    }
}

#[gpui::test]
fn query_widths_survive_same_query_and_do_not_follow_a_different_query(cx: &mut TestAppContext) {
    let source = |sql| {
        Arc::new(QuerySource {
            inner: WideSource(vec![ColumnMeta {
                name: "id".into(),
                width: 120.0,
                numeric: false,
                data_type: "integer".into(),
                primary_key: false,
            }]),
            sql,
            sql_reads: Arc::default(),
        })
    };
    cx.update(text_input::bind_keys);
    let (view, cx) =
        cx.add_window_view(|window, cx| GridView::with_source(source("select id from a"), Instant::now(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.grid_is_query = true;
            view.sync_table(window, cx);
            view.remember_grid_widths(&[gpui::px(180.0)], cx);
            view.source = source("select id from a");
            view.sync_table(window, cx);
            assert_eq!(f32::from(view.table.read(cx).col_width(0).unwrap()), 180.0);
            view.source = source("select id from b");
            view.sync_table(window, cx);
            assert_ne!(f32::from(view.table.read(cx).col_width(0).unwrap()), 180.0);
        })
    });
}

#[gpui::test]
fn rendering_the_same_query_result_reuses_its_width_identity(cx: &mut TestAppContext) {
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let source = Arc::new(QuerySource {
        inner: WideSource(vec![ColumnMeta {
            name: "id".into(),
            width: 120.0,
            numeric: true,
            data_type: "integer".into(),
            primary_key: false,
        }]),
        sql: "select id from a",
        sql_reads: reads.clone(),
    });
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.grid_is_query = true;
            view.sync_table(window, cx);
            let before = reads.load(std::sync::atomic::Ordering::Relaxed);
            for _ in 0..5 {
                view.sync_table(window, cx);
            }
            assert_eq!(reads.load(std::sync::atomic::Ordering::Relaxed), before);
        })
    });
}

#[gpui::test]
fn fitting_a_column_includes_its_type_header(cx: &mut TestAppContext) {
    let source = Arc::new(FitSource {
        columns: vec![ColumnMeta {
            name: "id".into(),
            width: 56.0,
            numeric: false,
            data_type: "varchar_with_a_long_type_name_that_exceeds_the_sample_cell".into(),
            primary_key: false,
        }],
        row: vec![Cell::Text("x".into())],
    });
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.simulate_resize(gpui::size(gpui::px(900.0), gpui::px(760.0)));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let menu = cx.update(|window, _| window.find_by_label("Actions for id").bounds());
    cx.simulate_click(menu.center(), Default::default());
    cx.run_until_parked();
    cx.update(|window, _| {
        assert!(window.try_find_by_label("Fit column to content").is_some());
        assert!(window.try_find_by_label("Reset column widths").is_some());
    });
    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();

    cx.update(|_, cx| {
        view.read_with(cx, |view, cx| {
            let width = f32::from(view.table.read(cx).col_width(0).expect("fitted column width"));
            assert!(width > 200.0, "type header should fit without ellipsis: {width}");
        })
    });
}

struct BinarySource(Vec<u8>);

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
        vec![Cell::Text("blob".into()), Cell::Bytes(self.0.clone())]
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
    let label = view.update(cx, |view, cx| {
        view.table.update(cx, |table, _| table.delegate_mut().label_of(0, 2).0)
    });
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
    let source = Arc::new(BinarySource(vec![0, 255, 65]));
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
        assert_eq!(
            view.table.update(cx, |table, _| table.delegate_mut().label_of(0, 1).0),
            "AP9B"
        );
        view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
        assert_eq!(
            view.detail_target().map(|(_, detail)| detail),
            Some(bookie_gpui::detail::Detail::Text("AP9B".into()))
        );
    });

    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
    view.read_with(cx, |view, cx| {
        assert_eq!(view.cell_edit.editor.read(cx).content(), "AP9B")
    });
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("bad=");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty());
        assert!(view.status.contains("not valid Base64 bytes"));
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("AP9C");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(0, 1)), Some(&Cell::Bytes(vec![0, 255, 66])));
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.source = Arc::new(BinarySource(vec![0, 255, 65]));
            view.table_source = view.source.clone();
            view.sync_table(window, cx);
        })
    });
    view.update(cx, |view, cx| {
        assert_eq!(
            view.table.update(cx, |table, _| table.delegate_mut().label_of(0, 1).0),
            "AP9C"
        );
    });
}

#[gpui::test]
fn an_untouched_multiline_or_large_binary_edit_stages_nothing(cx: &mut TestAppContext) {
    cx.update(text_input::bind_keys);
    let cases = [
        (
            b"first line\nsecond line".to_vec(),
            crate::binary_mode::BinaryMode::Utf8,
        ),
        (vec![7; 4096], crate::binary_mode::BinaryMode::Hex),
    ];
    for (bytes, mode) in cases {
        let source = Arc::new(BinarySource(bytes));
        let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
        cx.run_until_parked();
        view.update(cx, |view, cx| view.set_binary_mode(1, mode, cx));
        cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
        view.read_with(cx, |view, _| {
            assert!(
                view.cell_edit.active.as_ref().is_some_and(|editing| editing.full_value),
                "{mode:?}"
            );
        });
        cx.update(|window, cx| view.update(cx, |view, cx| view.commit_edit(window, cx)));
        view.read_with(cx, |view, _| {
            assert!(view.cell_edit.edits.is_empty(), "{mode:?}: {:?}", view.cell_edit.edits);
        });
    }
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
    let menu_bounds = cx.update(|window, _| window.find_by_label("Actions for column_1").bounds());
    cx.simulate_click(menu_bounds.center(), Default::default());
    cx.run_until_parked();
    let pin_bounds = cx.update(|window, _| window.find_by_label("Pin through column_1").bounds());
    cx.simulate_click(pin_bounds.center(), Default::default());
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
            view.sync_table(window, cx);
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
fn pinning_a_leading_column_keeps_the_last_cell_inside_the_scroll_viewport(cx: &mut TestAppContext) {
    let source = Arc::new(WideSource(
        (0..41)
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
    let pin_bounds = cx.update(|window, _| window.find_by_label("Pin through column_0").bounds());
    cx.simulate_click(pin_bounds.center(), Default::default());
    cx.run_until_parked();

    view.update(cx, |view, cx| {
        view.table.update(cx, |table, _| {
            let offset_past_scroll_end = gpui::point(gpui::px(-10_000.0), gpui::px(0.0));
            table.horizontal_scroll_handle.set_offset(offset_past_scroll_end);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));

    let viewport = cx
        .debug_bounds("grid-table-viewport")
        .expect("table viewport is rendered");
    let horizontal_scrollbar = cx
        .debug_bounds("grid-horizontal-scrollbar-track")
        .expect("horizontal scrollbar track is rendered");
    let vertical_gutter = cx
        .debug_bounds("grid-vertical-scrollbar-gutter")
        .expect("vertical scrollbar gutter is rendered");
    cx.update(|window, _| {
        let pinned = window.find(("col-header", 0usize)).bounds();
        let last_header = window.find(("col-header", 40usize)).bounds();
        let last_cell = window.find("table-cell-0:40").bounds();
        let visible_right = vertical_gutter.origin.x;
        assert!(
            pinned.origin.x >= viewport.origin.x
                && pinned.origin.x + pinned.size.width <= visible_right,
            "pinned leading header is outside the grid viewport: {pinned:?}, viewport: {viewport:?}"
        );
        let pinned_right = pinned.origin.x + pinned.size.width;
        assert!(
            horizontal_scrollbar.origin.x >= pinned_right - gpui::px(1.0)
                && horizontal_scrollbar.origin.x <= pinned_right + gpui::px(1.0),
            "scrollbar track should start after the pinned leading columns: track={:?}, pinned right={pinned_right:?}",
            horizontal_scrollbar.origin.x
        );
        assert!(
            last_header.origin.x + last_header.size.width <= visible_right,
            "pinned leading column leaves the last header clipped: {last_header:?}, visible right edge: {visible_right:?}"
        );
        assert!(
            last_cell.origin.x + last_cell.size.width <= visible_right,
            "pinned leading column leaves the last cell clipped: {last_cell:?}, visible right edge: {visible_right:?}"
        );
    });
}

#[gpui::test]
fn a_small_result_keeps_fitted_width_and_wide_headers_keep_pin_in_the_menu(cx: &mut TestAppContext) {
    let source = Arc::new(WideSource(vec![ColumnMeta {
        name: "category".into(),
        width: 150.0,
        numeric: false,
        data_type: "text".into(),
        primary_key: false,
    }]));
    cx.update(text_input::bind_keys);
    let (_, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.simulate_resize(gpui::size(gpui::px(950.0), gpui::px(760.0)));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let fitted = cx.debug_bounds("column-header-0").expect("fitted header");

    cx.simulate_resize(gpui::size(gpui::px(1_400.0), gpui::px(760.0)));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let header = cx.debug_bounds("column-header-0").expect("header after resize");
    assert_eq!(header.size.width, fitted.size.width);
    assert!(header.size.width < gpui::px(260.0));
    assert!(cx.update(|window, _| window.try_find_by_label("Pin through category").is_none()));

    let menu = cx.update(|window, _| window.find_by_label("Actions for category").bounds());
    assert!(header.origin.x + header.size.width - (menu.origin.x + menu.size.width) <= gpui::px(2.0));
    cx.simulate_click(menu.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.try_find_by_label("Pin through category").is_some()));
}

#[gpui::test]
fn fitting_visible_columns_preserves_the_width_of_hidden_columns(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.0), gpui::px(800.0)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.remember_grid_widths(&[gpui::px(320.0); 4], cx);
            view.hide_column(1, "label", window, cx);
            view.fit_visible_grid_columns(window, cx);
            view.show_all_hidden_columns(window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let table = view.table.read(cx);
        assert_eq!(table.col_width(1), Some(gpui::px(320.0)));
        assert!(table.col_width(0).unwrap() < gpui::px(320.0));
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
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.simulate_resize(gpui::size(gpui::px(1_896.0), gpui::px(1_150.0)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.remember_grid_widths(&[gpui::px(72.0); 40], cx));
    });
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
        view.table
            .update(cx, |table, cx| table.set_selected_cell(4_999, 59, cx));
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
