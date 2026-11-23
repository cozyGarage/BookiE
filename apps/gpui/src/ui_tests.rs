use std::time::Instant;

use bookie_gpui::model::Cell;
use gpui::{TestAppContext, VisualTestContext};

use bookie_results::grid_cursor::Cursor;

use crate::palette_state::PaletteMode;
use crate::text_input;
use crate::ui_theme::ThemeExt;
use crate::{GridView, Work};

mod failed_runs;

pub(super) fn open(cx: &mut TestAppContext) -> (gpui::Entity<GridView>, &mut VisualTestContext) {
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::new(150_000, Instant::now(), window, cx));
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.run_until_parked();
    (view, cx)
}

#[gpui::test]
fn opening_the_filter_keeps_grid_controls_on_one_line_at_default_width(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(1100.0), gpui::px(800.0)));
    cx.update(|_, cx| view.update(cx, |view, cx| {
        view.search.filter_visible = true;
        cx.notify();
    }));
    cx.run_until_parked();
    let pager = cx.debug_bounds("grid-pager").expect("pager is rendered");
    let actions = cx.debug_bounds("grid-actions").expect("actions are rendered");
    assert!(
        pager.origin.y < actions.origin.y + actions.size.height
            && actions.origin.y < pager.origin.y + pager.size.height,
        "pager and grid actions should share a row"
    );
}

#[gpui::test]
fn a_valid_edit_is_applied_to_the_typed_cell_and_marked(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(2, 3, window, cx)));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("2026-02-03");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cell_edit.edits.get(&(2, 3)),
            Some(&Cell::Date("2026-02-03".parse().unwrap()))
        );
        assert!(view.cell_edit.active.is_none());
        assert!(view.status.contains("edited in this view only"));
    });
}

#[gpui::test]
fn undo_and_redo_step_through_the_staged_edits_of_a_cell(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let edit = |cx: &mut gpui::VisualTestContext, text: &str| {
        cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(2, 3, window, cx)));
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input(text);
        cx.simulate_keystrokes("enter");
    };
    let date = |text: &str| Some(Cell::Date(text.parse().unwrap()));
    edit(cx, "2026-02-03");
    edit(cx, "2026-03-04");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(2, 3)).cloned(), date("2026-03-04"))
    });

    cx.simulate_keystrokes("ctrl-z");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cell_edit.edits.get(&(2, 3)).cloned(),
            date("2026-02-03"),
            "back to the first edit"
        );
        assert_eq!(view.status, "Undid an edit");
    });
    cx.simulate_keystrokes("ctrl-z");
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty(), "back to the original value")
    });
    cx.simulate_keystrokes("ctrl-z");
    view.read_with(cx, |view, _| assert_eq!(view.status, "Nothing to undo"));

    cx.simulate_keystrokes("ctrl-y ctrl-y");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(2, 3)).cloned(), date("2026-03-04"))
    });
    cx.simulate_keystrokes("ctrl-y");
    view.read_with(cx, |view, _| assert_eq!(view.status, "Nothing to redo"));

    edit(cx, "2026-05-06");
    cx.simulate_keystrokes("ctrl-y");
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Nothing to redo", "a new edit drops the redo history");
    });
}

#[gpui::test]
fn closing_a_run_forgets_the_undo_history_so_redo_cannot_stage_an_edit_in_another_grid(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.add_run(None);
        })
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(2, 3, window, cx)));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("2026-02-03");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("ctrl-z");
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty());
        assert_eq!(view.cell_edit.redo.len(), 1, "the undone edit waits to be redone");
    });

    cx.update(|_, cx| view.update(cx, |view, _| view.close_run(1)));
    view.read_with(cx, |view, _| {
        assert_eq!(view.run_tabs.slots.len(), 1, "the active run closed: {}", view.status)
    });
    cx.simulate_keystrokes("ctrl-y");
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Nothing to redo", "the old grid's history is gone");
        assert!(
            view.cell_edit.edits.is_empty(),
            "no value from the closed result was staged"
        );
    });
}

#[gpui::test]
fn the_undo_history_is_bounded_by_count_and_by_size(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            for n in 0..300 {
                view.record_edit((1, 1), Cell::Text(format!("edit {n}")));
            }
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cell_edit.undo.len(),
            200,
            "only the latest 200 edits can be undone"
        );
        assert_eq!(
            view.cell_edit.edits.get(&(1, 1)),
            Some(&Cell::Text("edit 299".into())),
            "trimming the history never changes the staged value"
        );
    });
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.reset_edit_history();
            for _ in 0..3 {
                view.record_edit((2, 1), Cell::Text("x".repeat(3 * 1024 * 1024)));
            }
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cell_edit.undo.len(),
            1,
            "large values cannot pile up: only the latest edit is kept"
        );
    });
}

#[gpui::test]
fn confirming_an_untouched_null_cell_keeps_it_null(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let row = view
        .read_with(cx, |view, _| (0..12).find(|row| matches!(view.cell_at(*row, 2), Cell::Null)))
        .expect("the synthetic data has a NULL state");
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(row, 2, window, cx)));
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty(), "Enter on an untouched NULL staged an edit");
        assert!(view.cell_edit.active.is_none());
    });
}

#[test]
fn editing_a_cell_does_not_write_its_value_to_process_output() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ui_tests::a_valid_edit_is_applied_to_the_typed_cell_and_marked",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("2026-02-03"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("2026-02-03"));
}

#[gpui::test]
fn opening_a_query_reveals_its_tab_in_an_overflowing_workspace(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            for index in 0..15 {
                view.tabs
                    .open_table(&bookie_gpui::shell::table_info(&format!("table_{index}")));
            }
            cx.notify();
        });
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-t");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::Query);
        assert!(view.tab_scroll.offset().x < gpui::px(0.0));
    });
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("open home");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::Home);
        assert_eq!(view.tab_scroll.offset().x, gpui::px(0.0));
    });
}

#[gpui::test]
fn an_invalid_edit_is_refused_and_keeps_the_editor_open(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(2, 3, window, cx)));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("2026-99-99");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty());
        assert!(view.cell_edit.active.is_some());
        assert_eq!(view.status, "Not applied: not a date");
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.cell_edit.active.is_none() && view.cell_edit.edits.is_empty()));
}

#[gpui::test]
fn the_undecodable_cell_cannot_be_edited(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(10, 3, window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.active.is_none());
        assert_eq!(view.status, "This cell cannot be edited");
    });
}

#[gpui::test]
fn the_find_field_takes_typed_text_and_enter_runs_the_search(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        let handle = view.read(cx).search.find.read(cx).focus_handle_for_tests();
        window.focus(&handle, cx);
    });
    cx.simulate_input("sample 149999");
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.search.find.read(cx).content(), "sample 149999");
        assert_eq!(view.search.cursor.matches, vec![149_999]);
    });
}

#[gpui::test]
fn changing_find_text_discards_a_pending_search_result(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.search.find.update(cx, |input, cx| input.set_content("Sample 7", cx));
            view.run_find(true, cx);
        });
    });
    let cancel = view.read_with(cx, |view, _| view.scans.cancel.clone());
    view.update(cx, |view, cx| {
        view.search.find.update(cx, |input, cx| input.set_content("new search", cx))
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cancel.is_cancelled());
    view.read_with(cx, |view, _| assert!(view.search.cursor.matches.is_empty()));
}

fn flag() -> (std::rc::Rc<std::cell::Cell<bool>>, std::rc::Rc<std::cell::Cell<bool>>) {
    let flag = std::rc::Rc::new(std::cell::Cell::new(false));
    (flag.clone(), flag)
}

#[gpui::test]
fn starting_a_find_does_not_discard_a_pending_load(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (applied, mark) = flag();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.begin_background(Work::Load, cx, |_| (), move |_, ()| mark.set(true));
            view.search.find.update(cx, |input, cx| input.set_content("Sample 7", cx));
            view.run_find(true, cx);
            assert!(!view.loads.cancel.is_cancelled(), "a find must not cancel the load");
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(applied.get(), "the load result was dropped by a find");
}

#[gpui::test]
fn applying_a_filter_does_not_discard_a_pending_load(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (applied, mark) = flag();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.begin_background(Work::Load, cx, |_| (), move |_, ()| mark.set(true));
            view.search.filter
                .update(cx, |input, cx| input.set_content("label contains 7", cx));
            view.apply_filter(cx);
            assert!(!view.loads.cancel.is_cancelled(), "a filter must not cancel the load");
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(applied.get(), "the load result was dropped by a filter");
}

#[gpui::test]
fn starting_a_load_discards_a_pending_scan(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (applied, mark) = flag();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.begin_background(Work::Scan, cx, |_| (), move |_, ()| mark.set(true));
            let scan = view.scans.cancel.clone();
            view.begin_background(Work::Load, cx, |_| (), |_, ()| {});
            assert!(scan.is_cancelled(), "a scan of the old data must be cancelled");
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(!applied.get(), "a scan result for replaced data was applied");
}

#[gpui::test]
fn changing_filter_text_discards_a_pending_filter(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.search.filter
                .update(cx, |input, cx| input.set_content("label contains 7", cx));
            view.apply_filter(cx);
        });
    });
    let cancel = view.read_with(cx, |view, _| view.scans.cancel.clone());
    view.update(cx, |view, cx| {
        view.search.filter
            .update(cx, |input, cx| input.set_content("label contains 8", cx))
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cancel.is_cancelled());
    view.read_with(cx, |view, _| {
        assert!(matches!(view.mode, crate::Mode::All));
        assert_ne!(view.status, "Filtering…");
    });
}

#[gpui::test]
fn the_arrow_keys_move_the_active_cell_and_space_selects_its_row(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("down down right");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_cursor, Some(Cursor { row: 2, column: 1 }));
        assert_eq!(view.status, "Row 3, Label");
    });
    cx.simulate_keystrokes("space");
    view.read_with(cx, |view, _| assert_eq!(view.selection.map(|s| s.count()), Some(1)));
    cx.simulate_keystrokes("shift-down shift-down");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_cursor, Some(Cursor { row: 4, column: 1 }));
        assert_eq!(view.selection.map(|s| s.count()), Some(3));
        assert_eq!(view.selection.map(|s| s.anchor), Some(2));
        assert_eq!(view.selection.map(|s| s.focus), Some(4));
        assert_eq!(view.status, "3 rows selected");
    });
}

#[gpui::test]
fn shift_arrow_seeds_a_range_from_the_active_cell(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("down down");
    view.read_with(cx, |view, _| {
        assert!(view.selection.is_none());
        assert_eq!(view.cell_cursor, Some(Cursor { row: 2, column: 0 }));
    });
    cx.simulate_keystrokes("shift-down");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_cursor, Some(Cursor { row: 3, column: 0 }));
        assert_eq!(view.selection.map(|s| s.count()), Some(2));
        assert_eq!(view.selection.map(|s| s.anchor), Some(2));
        assert_eq!(view.selection.map(|s| s.focus), Some(3));
    });
}

#[gpui::test]
fn an_arrow_past_the_page_edge_takes_the_neighbouring_page(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.place_cursor(Some(Cursor { row: 4_998, column: 3 }));
            assert!(!view.cross_page_edge("down"), "not the last row yet");
            view.place_cursor(Some(Cursor { row: 4_999, column: 3 }));
            assert!(view.cross_page_edge("down"));
            assert_eq!(view.pager.page, 1);
            assert_eq!(view.cell_cursor, Some(Cursor { row: 0, column: 3 }));
            assert!(view.cross_page_edge("up"));
            assert_eq!(view.pager.page, 0);
            assert_eq!(view.cell_cursor, Some(Cursor { row: 4_999, column: 3 }));
            assert!(!view.cross_page_edge("left"));
        })
    });
}

#[gpui::test]
fn the_cursor_crosses_to_the_next_page_and_back(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-end");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_cursor, Some(Cursor { row: 4_999, column: 3 }));
        assert_eq!(view.pager.page, 0);
    });
    cx.simulate_keystrokes("down");
    view.read_with(cx, |view, _| {
        assert_eq!(view.pager.page, 1);
        assert_eq!(view.cell_cursor, Some(Cursor { row: 0, column: 3 }));
    });
    cx.simulate_keystrokes("up");
    view.read_with(cx, |view, _| {
        assert_eq!(view.pager.page, 0);
        assert_eq!(view.cell_cursor, Some(Cursor { row: 4_999, column: 3 }));
    });
}

#[gpui::test]
fn enter_on_the_active_cell_opens_the_editor_and_an_uneditable_cell_says_so(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("down right enter");
    view.read_with(cx, |view, _| {
        let editing = view.cell_edit.active.as_ref().expect("the label cell opens its editor");
        assert_eq!((editing.row, editing.column), (1, 1));
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("home enter");
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.active.is_none());
        assert_eq!(view.status, "This cell cannot be edited");
    });
}

#[gpui::test]
fn the_palette_filters_runs_and_closes_on_escape(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    view.read_with(cx, |view, _| assert!(view.palette.open));
    cx.simulate_input("last page");
    view.read_with(cx, |view, cx| {
        let labels: Vec<_> = view
            .palette_shown(cx)
            .into_iter()
            .map(|index| view.palette_actions()[index].label.clone())
            .collect();
        assert_eq!(labels, ["Go to last page"]);
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(!view.palette.open);
        assert_eq!(view.pager.page, 29);
    });
    cx.simulate_keystrokes("ctrl-shift-p escape");
    view.read_with(cx, |view, _| assert!(!view.palette.open));
}

#[gpui::test]
fn the_quick_switcher_lists_tables_filters_them_and_opens_the_chosen_one(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-p");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.palette.input.read(cx).placeholder(), "Go to a table or favorite");
        assert!(view.palette.open && view.palette.mode == PaletteMode::Tables);
        assert_eq!(view.picker_items(cx).len(), view.catalog.tables.len());
    });
    cx.simulate_input("cust");
    view.read_with(cx, |view, cx| {
        let titles: Vec<_> = view.picker_items(cx).into_iter().map(|item| item.title).collect();
        assert_eq!(titles, ["customers"]);
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(!view.palette.open && view.palette.mode == PaletteMode::Commands);
        let customers = bookie_gpui::shell::table_info("customers");
        assert_eq!(*view.tabs.current(), bookie_gpui::shell::Tab::Table(customers));
    });
    cx.simulate_keystrokes("ctrl-shift-p");
    view.read_with(cx, |view, _| {
        assert!(
            view.palette.open && view.palette.mode == PaletteMode::Commands,
            "the command palette is separate"
        );
    });
}

#[gpui::test]
fn the_quick_switcher_lists_favorites_first_and_opens_one_in_the_editor(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.favorites.list = vec![bookie_app_services::favorites::Favorite {
                id: uuid::Uuid::new_v4(),
                name: "Big orders".into(),
                sql: "SELECT * FROM orders WHERE total > 100".into(),
            }];
        })
    });
    cx.simulate_keystrokes("ctrl-p");
    view.read_with(cx, |view, cx| {
        let rows: Vec<_> = view
            .picker_items(cx)
            .into_iter()
            .map(|item| (item.title, item.subtitle))
            .collect();
        assert_eq!(rows[0], ("Big orders".to_string(), "Favorite".to_string()));
        assert_eq!(rows.len(), 1 + view.catalog.tables.len());
    });
    cx.simulate_input("big");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, cx| {
        assert_eq!(*view.tabs.current(), bookie_gpui::shell::Tab::Query);
        assert_eq!(
            view.sql_input.read(cx).value(),
            "SELECT * FROM orders WHERE total > 100"
        );
    });
}

#[gpui::test]
fn jump_to_column_lists_the_columns_and_moves_the_cursor_to_the_chosen_one(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("down down");
    cx.simulate_keystrokes("ctrl-shift-j");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.palette.mode, PaletteMode::Columns);
        assert_eq!(view.picker_items(cx).len(), view.source.columns().len());
    });
    cx.simulate_input("lab");
    view.read_with(cx, |view, cx| {
        let titles: Vec<_> = view.picker_items(cx).into_iter().map(|item| item.title).collect();
        assert_eq!(titles, ["label"]);
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.palette.mode, PaletteMode::Commands);
        let cursor = view.cell_cursor.expect("the cursor stays on a cell");
        assert_eq!((cursor.row, cursor.column), (2, 1), "same row, the label column");
    });
}

#[gpui::test]
fn reveal_in_sidebar_opens_the_sidebar_clears_the_search_and_expands_the_tables_schema(cx: &mut TestAppContext) {
    use bookie_results::sidebar_tree::{NodeKind, build_nodes};
    let (view, cx) = open(cx);
    let table = bookie_core::TableInfo {
        schema: Some("app".into()),
        name: "customers".into(),
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let other = bookie_core::TableInfo {
                schema: Some("other".into()),
                name: "orders".into(),
            };
            view.catalog.tables = vec![table.clone(), other];
            let nodes = build_nodes(&view.catalog.tables, &[]);
            let schema = nodes
                .iter()
                .find(|node| node.kind == NodeKind::Schema && node.schema.as_deref() == Some("app"))
                .expect("the app schema node");
            view.sidebar.collapsed_schemas.toggle(&schema.key);
            view.sidebar.open = false;
            view.sidebar.search.update(cx, |input, cx| input.set_content("zzz", cx));
            view.tabs.open_table(&table);
            view.open_table_named(&table, window, cx);
        })
    });
    cx.simulate_keystrokes("ctrl-shift-l");
    view.read_with(cx, |view, cx| {
        assert!(view.sidebar.open, "the sidebar opens");
        assert_eq!(view.sidebar.search.read(cx).content(), "", "the search is cleared");
        assert!(
            view.sidebar.collapsed_schemas.keys().is_empty(),
            "the schema is expanded"
        );
        assert_eq!(view.status, "Revealed app.customers in the sidebar");
    });
}

#[gpui::test]
fn the_shortcut_list_shows_every_command_with_its_keys_and_runs_the_chosen_one(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("f1");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.palette.mode, PaletteMode::Shortcuts);
        assert_eq!(view.picker_items(cx).len(), bookie_gpui::keymap::Command::ALL.len());
    });
    cx.simulate_input("column");
    view.read_with(cx, |view, cx| {
        let rows: Vec<_> = view
            .picker_items(cx)
            .into_iter()
            .map(|item| (item.title, item.subtitle))
            .collect();
        assert_eq!(rows, [("Jump to column".to_string(), "ctrl-shift-j".to_string())]);
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.palette.mode, PaletteMode::Columns, "the chosen command ran");
    });
}

#[gpui::test]
fn a_long_sidebar_scrolls_and_reveal_brings_the_open_table_into_view(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let tables: Vec<_> = (0..80)
        .map(|n| bookie_gpui::shell::table_info(&format!("t{n:02}")))
        .collect();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.catalog.tables = tables.clone();
            view.sidebar.open = true;
            view.tabs.open_table(&tables[70]);
            view.open_table_named(&tables[70], window, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.sidebar.scroll.is_scrollable(), "the tree overflows and can scroll");
    });
    assert!(cx.debug_bounds("sidebar-row-0").is_some(), "the first row is drawn");
    assert!(
        cx.debug_bounds("sidebar-row-72").is_none(),
        "rows below the fold are not built"
    );
    cx.simulate_keystrokes("ctrl-shift-l");
    cx.run_until_parked();
    assert!(cx.debug_bounds("sidebar-row-72").is_some(), "row 72 (t70) is revealed");
}

#[gpui::test]
fn favorites_are_saved_from_the_editor_listed_loaded_and_deleted(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let file = std::env::temp_dir().join(format!("bookie-gpui-favorites-{}.json", uuid::Uuid::new_v4()));
    let settle = |cx: &mut gpui::VisualTestContext| {
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.favorites.path = Some(file.clone());
            view.tabs.open_query();
            view.sync_grid_owner();
            view.sql_input
                .update(cx, |input, cx| input.set_value("SELECT 1", window, cx));
        })
    });
    cx.simulate_keystrokes("ctrl-d");
    settle(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Saved favorite \u{201c}SELECT 1\u{201d}");
        assert_eq!(view.favorites.list.len(), 1);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sql_input.update(cx, |input, cx| input.set_value("", window, cx));
        })
    });
    cx.simulate_keystrokes("ctrl-shift-b");
    settle(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.palette.mode, PaletteMode::Favorites);
        let titles: Vec<_> = view.picker_items(cx).into_iter().map(|item| item.title).collect();
        assert_eq!(titles, ["SELECT 1"]);
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.sql_input.read(cx).value(),
            "SELECT 1",
            "the favorite fills the editor"
        );
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.run_palette_id("delete-favorite", window, cx)));
    settle(cx);
    view.read_with(cx, |view, _| assert_eq!(view.palette.mode, PaletteMode::DeleteFavorite));
    cx.simulate_keystrokes("enter");
    settle(cx);
    view.read_with(cx, |view, _| {
        assert!(view.favorites.list.is_empty(), "the favorite is gone");
        assert!(view.status.starts_with("Deleted favorite"), "{}", view.status);
    });
    std::fs::remove_file(&file).ok();
}

#[gpui::test]
fn the_palette_hides_actions_that_do_not_apply_and_arrows_pick_the_next_one(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    view.read_with(cx, |view, cx| {
        let ids: Vec<_> = view
            .palette_shown(cx)
            .into_iter()
            .map(|index| view.palette_actions()[index].id)
            .collect();
        assert!(!ids.contains(&"clear-filter") && !ids.contains(&"copy"));
        assert!(ids.contains(&"edit"));
        assert!(ids.contains(&"last"));
    });
    cx.simulate_keystrokes("down");
    view.read_with(cx, |view, _| assert_eq!(view.palette.index, 1));
    cx.simulate_keystrokes("up up");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.palette.index, view.palette_shown(cx).len() - 1)
    });
}

#[gpui::test]
fn the_palette_switches_between_the_light_and_dark_theme(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| assert!(!bookie_gpui::theme::Theme::current(cx).dark, "light is the default"));
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("dark theme");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(bookie_gpui::theme::Theme::current(cx).dark));
    view.read_with(cx, |view, _| assert!(view.dark_mode));
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("light theme");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(!bookie_gpui::theme::Theme::current(cx).dark));
}

#[gpui::test]
fn ctrl_b_hides_and_shows_the_sidebar(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.read_with(cx, |view, _| assert!(view.sidebar.open));
    cx.simulate_keystrokes("ctrl-b");
    view.read_with(cx, |view, _| assert!(!view.sidebar.open));
    cx.simulate_keystrokes("ctrl-b");
    view.read_with(cx, |view, _| assert!(view.sidebar.open));
}

#[gpui::test]
fn the_home_tab_has_no_grid_so_arrow_keys_do_nothing_until_the_table_tab_is_back(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("open home");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert_eq!(view.tabs.active, 0));
    let before = view.read_with(cx, |view, _| view.cell_cursor);
    cx.simulate_keystrokes("down down");
    view.read_with(cx, |view, _| assert_eq!(view.cell_cursor, before));
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("open synthetic");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert!(view.tabs.shows_grid()));
    cx.simulate_keystrokes("down");
    view.read_with(cx, |view, _| assert_eq!(view.cell_cursor.map(|c| c.row), Some(1)));
}

#[gpui::test]
fn the_sidebar_search_takes_typed_text_and_filters_the_tree(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        let handle = view.read(cx).sidebar.search.read(cx).focus_handle_for_tests();
        window.focus(&handle, cx);
    });
    cx.simulate_input("aud");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.sidebar.search.read(cx).content(), "aud");
        let rows = bookie_gpui::shell::tree_rows("aud", true, true);
        assert_eq!(rows.len(), 3, "connection, database and the one matching table");
    });
}

#[gpui::test]
fn the_sidebar_locate_regex_and_active_only_controls_filter_the_tree(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.sidebar.open = false;
            view.locate_active_table(cx);
            assert!(view.sidebar.open);
            assert_eq!(view.sidebar.search.read(cx).content(), "synthetic_rows");
            assert!(view.status.starts_with("Located "));
            view.toggle_sidebar_regex(cx);
            assert!(view.sidebar.regex);
            view.sidebar
                .search
                .update(cx, |input, cx| input.set_content(r"^cust", cx));
            let rows = bookie_gpui::shell::tree_rows_in(
                &view.catalog,
                r"^cust",
                true,
                true,
                bookie_gpui::shell::SidebarFilter {
                    regex: true,
                    active_only: false,
                },
            );
            assert_eq!(
                rows.iter()
                    .filter(|(_, row)| matches!(row, bookie_gpui::shell::TreeRow::Table(_)))
                    .count(),
                1
            );
            view.sidebar.active_only = true;
            let active = bookie_gpui::shell::tree_rows_in(
                &view.catalog,
                "",
                true,
                true,
                bookie_gpui::shell::SidebarFilter {
                    regex: false,
                    active_only: true,
                },
            );
            assert_eq!(
                active
                    .iter()
                    .filter_map(|(_, row)| match row {
                        bookie_gpui::shell::TreeRow::Table(table) => Some(table.name.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                ["synthetic_rows"]
            );
        })
    });
}

#[gpui::test]
fn the_palette_lists_installed_fonts_and_the_chosen_one_becomes_the_ui_font(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.fonts.names = vec!["Foo Sans".into(), "Bar Mono".into()]
        })
    });
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("choose font");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, cx| {
        assert!(
            view.palette.open && view.palette.fonts,
            "the palette stays open on the font list"
        );
        let labels: Vec<_> = view
            .palette_shown(cx)
            .into_iter()
            .map(|index| view.palette_actions()[index].label.clone())
            .collect();
        assert_eq!(labels, ["Foo Sans", "Bar Mono"]);
    });
    cx.simulate_input("bar");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.fonts.current.as_ref(), "Bar Mono");
        assert!(!view.palette.open && !view.palette.fonts);
    });
}

#[gpui::test]
fn the_palette_switches_between_light_and_dark(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("dark theme");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(bookie_gpui::theme::Theme::current(cx).dark));
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("light theme");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(!bookie_gpui::theme::Theme::current(cx).dark));
}

#[gpui::test]
fn the_overflow_menu_lists_commands_by_group_and_runs_them(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let groups = view.read_with(cx, |view, _| {
        crate::menus::command_groups(view, crate::menus::OVERFLOW_GROUPS)
    });
    let ids: Vec<Vec<&str>> = groups
        .iter()
        .map(|group| group.iter().map(|(id, _, _)| *id).collect())
        .collect();
    assert_eq!(ids[0], ["query", "run", "explain", "data-dictionary"]);
    assert!(ids.iter().flatten().any(|id| *id == "settings"));
    let open_before = view.read_with(cx, |view, _| view.sidebar.open);
    cx.update(|window, cx| view.update(cx, |view, cx| view.run_palette_id("sidebar", window, cx)));
    assert_ne!(view.read_with(cx, |view, _| view.sidebar.open), open_before);
}

#[gpui::test]
fn the_palette_opens_one_new_connection_tab_with_the_default_port(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for _ in 0..2 {
        cx.simulate_keystrokes("ctrl-shift-p");
        cx.simulate_input("new connection");
        cx.simulate_keystrokes("enter");
    }
    view.read_with(cx, |view, cx| {
        let tabs = view
            .tabs
            .items
            .iter()
            .filter(|tab| **tab == bookie_gpui::shell::Tab::NewConnection)
            .count();
        assert_eq!(tabs, 1);
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::NewConnection);
        let draft = view.connection_form.draft(cx);
        assert_eq!((draft.driver_id.as_str(), draft.port), ("postgres", 5432));
        assert!(!draft.capabilities.is_file_based);
        assert!(draft.read_only);
    });
    view.update(cx, |view, cx| {
        view.connection_form.port.update(cx, |input, cx| input.set_content("not-a-port", cx));
        assert_eq!(view.connection_form.draft(cx).port, 0);
    });
    let sqlite = view.read_with(cx, |view, _| {
        view.connection_form
            .drivers
            .iter()
            .position(|driver| driver.id == "sqlite")
            .expect("SQLite is registered")
    });
    view.update(cx, |view, cx| {
        view.connection_form.driver = sqlite;
        assert!(view.connection_form.draft(cx).capabilities.is_file_based);
    });
}

#[gpui::test]
fn palette_end_keeps_the_last_command_in_view(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.run_until_parked();
    cx.simulate_keystrokes("end");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let count = view.palette_shown(cx).len();
        assert!(count > 20);
        assert_eq!(view.palette.index, count - 1);
        assert_eq!(view.palette.scroll.is_scrolled_to_end(), Some(true));
    });
    let count = view.read_with(cx, |view, cx| view.palette_shown(cx).len());
    // debug_bounds takes a static selector; leaking one short string per test run is fine.
    let last: &'static str = Box::leak(format!("palette-command-row-{}", count - 1).into_boxed_str());
    let last = cx.debug_bounds(last).expect("last command is drawn");
    let height = cx.update(|window, _| window.viewport_size().height);
    assert!(last.bottom() <= height, "last command ends inside the window");
    assert!(
        cx.debug_bounds("palette-command-row-0").is_none(),
        "commands scrolled out of view are not built"
    );
}

#[gpui::test]
fn connection_form_keeps_actions_visible_at_two_widths(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.tabs.open_new_connection();
        cx.notify();
    });
    for width in [720.0, 1100.0] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(760.0)));
        cx.run_until_parked();
        let page = cx.debug_bounds("connection-page").expect("connection form is rendered");
        let actions = cx.debug_bounds("connection-actions").expect("connection actions are rendered");
        assert!(page.size.width > gpui::px(200.0));
        assert!(actions.origin.y >= page.origin.y);
        assert!(actions.origin.y + actions.size.height <= gpui::px(760.0));
    }
}

struct Tiny;

struct CountingSource(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl bookie_gpui::source::ResultSource for CountingSource {
    fn editable(&self) -> bool {
        true
    }

    fn columns(&self) -> &[bookie_gpui::model::ColumnMeta] {
        test_columns()
    }

    fn total(&self) -> u64 {
        3
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        bookie_gpui::source::ResultSource::row(&Tiny, index)
    }

    fn find(
        &self,
        query: &str,
        cap: usize,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<bookie_gpui::model::FindResult, &'static str> {
        bookie_gpui::source::ResultSource::find(&Tiny, query, cap, cancel)
    }

    fn filter(
        &self,
        predicate: &bookie_gpui::model::Predicate,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<Vec<u32>, &'static str> {
        bookie_gpui::source::ResultSource::filter(&Tiny, predicate, cancel)
    }

    fn compare(
        &self,
        keyed: bool,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<bookie_gpui::model::Diff, &'static str> {
        bookie_gpui::source::ResultSource::compare(&Tiny, keyed, cancel)
    }
}

#[gpui::test]
fn painting_a_row_reads_the_source_once_and_keeps_pending_edits_available(cx: &mut TestAppContext) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let requests = std::sync::Arc::new(AtomicUsize::new(0));
    let source = std::sync::Arc::new(CountingSource(requests.clone()));
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    cx.run_until_parked();
    let full = "💾".repeat(10_000);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.insert((1, 1), Cell::Text(full.clone()));
            requests.store(0, Ordering::Relaxed);
            view.sync_table(0.0, window, cx);
            view.table.update(cx, |table, _| {
                let delegate = table.delegate_mut();
                let edited = delegate.label_of(1, 1).0;
                assert_eq!(edited, bookie_gpui::preview::cell_preview(&Cell::Text(full.clone())));
                assert_ne!(edited, delegate.label_of(1, 0).0);
            });
            assert_eq!(requests.load(Ordering::Relaxed), 1);
        });
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(1, 1, window, cx)));
    view.read_with(cx, |view, cx| assert_eq!(view.cell_edit.editor.read(cx).content(), full));
}

#[gpui::test]
fn grid_table_shares_wide_viewport_space_without_widening_numeric_columns(cx: &mut TestAppContext) {
    let source = std::sync::Arc::new(bookie_gpui::source::Synthetic::new(1));
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));
    let mut previous_text_width = 0.0;
    for width in [800.0, 1_200.0] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(760.0)));
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            let table = view.table.read(cx);
            let widths: Vec<f32> = (0..4)
                .map(|column| f32::from(table.col_width(column).expect("synthetic column is rendered")))
                .collect();
            if width == 1_200.0 {
                assert!(widths.iter().sum::<f32>() <= width);
            }
            assert!(widths[1] <= 360.0 && widths[3] <= 360.0);
            assert!(widths[1] + widths[3] >= previous_text_width);
            previous_text_width = widths[1] + widths[3];
        });
    }
}

#[gpui::test]
fn copying_a_filtered_row_includes_its_full_pending_edit(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let value = "full\tvalue\nwith unicode 💾";
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_mode(crate::Mode::Filtered {
                kept: std::sync::Arc::new(vec![10, 20]),
                label: "filtered".into(),
            });
            view.cell_edit.edits.insert((20, 1), Cell::Text(value.into()));
            view.click_row(1, false);
            view.copy_selection(false, cx);
        });
    });
    let actual = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    let mut row = bookie_gpui::model::row_at(20);
    row[1] = Cell::Text(value.into());
    let columns = bookie_gpui::core_bridge::column_infos_for(&bookie_gpui::model::synthetic_columns());
    let expected = bookie_app_services::result_copy::tsv(&columns, vec![row], false);
    assert_eq!(actual.as_deref(), Some(expected.as_str()));
}

#[gpui::test]
fn local_sort_keeps_selection_and_pending_edits_attached_to_source_rows(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let value = "edited source row";
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_mode(crate::Mode::Filtered {
                kept: std::sync::Arc::new(vec![0, 2, 4]),
                label: "filtered".into(),
            });
            view.local_order = Some(std::sync::Arc::new(vec![4, 0, 2]));
            view.cell_edit.edits.insert((0, 1), Cell::Text(value.into()));
            view.click_row(1, false);
            view.copy_selection(false, cx);
        });
    });
    let actual = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    let mut row = bookie_gpui::model::row_at(0);
    row[1] = Cell::Text(value.into());
    let columns = bookie_gpui::core_bridge::column_infos_for(&bookie_gpui::model::synthetic_columns());
    let expected = bookie_app_services::result_copy::tsv(&columns, vec![row], false);
    assert_eq!(actual.as_deref(), Some(expected.as_str()));
}

#[gpui::test]
fn comparing_results_clears_selection_from_the_previous_grid(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.click_row(4, false);
            view.compare(false, cx);
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.compare.is_some());
        assert!(view.selection.is_none());
    });
}

#[gpui::test]
fn rendering_oversized_pending_cells_keeps_exact_binary_json_and_edit_text(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let text = format!("{}\0tail", "💾".repeat(8_192));
    let bytes = vec![0xab; 1_000_000];
    let json = format!("{{\"raw\":  \"{}\"}}", "x".repeat(1_000_000));
    view.update(cx, |view, cx| {
        view.tabs
            .open_table(&bookie_gpui::shell::table_info(bookie_gpui::shell::DATA_TABLE));
        view.cell_edit.edits.insert((0, 1), Cell::Text(text.clone()));
        view.cell_edit.edits.insert((0, 3), Cell::Bytes(bytes.clone()));
        view.cell_edit.edits.insert((1, 3), Cell::JsonText(json.clone()));
        cx.notify();
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_at(0, 3), Cell::Bytes(bytes));
        assert_eq!(view.cell_at(1, 3), Cell::JsonText(json));
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
    view.read_with(cx, |view, cx| assert_eq!(view.cell_edit.editor.read(cx).content(), text));
}

#[gpui::test]
fn opening_a_multiline_value_preserves_it_and_reports_the_single_line_editor_limit(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let value = Cell::Text("first\nsecond".into());
    view.update(cx, |view, _| {
        view.cell_edit.edits.insert((0, 1), value.clone());
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.active.is_none());
        assert_eq!(view.cell_edit.edits.get(&(0, 1)), Some(&value));
        assert!(!view.status.is_empty());
    });
}

#[gpui::test]
fn composed_text_selects_inside_its_replacement_after_a_unicode_prefix(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        let input = view.read(cx).search.find.clone();
        input.update(cx, |input, cx| {
            input.set_content("α OLD tail", cx);
            input.replace_and_mark_text_in_range(Some(2..5), "💾é", Some(2..3), window, cx);
            assert_eq!(input.content(), "α 💾é tail");
            assert_eq!(input.selected_text_range(false, window, cx).unwrap().range, 4..5);
            input.replace_text_in_range(None, "X", window, cx);
            assert_eq!(input.content(), "α X tail");
        });
    });
    cx.run_until_parked();
}

#[gpui::test]
fn platform_input_flattens_newlines_like_single_line_paste(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        let focus = view.read(cx).search.find.read(cx).focus_handle_for_tests();
        window.focus(&focus, cx);
    });
    cx.simulate_input("first\nsecond");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| assert_eq!(view.search.find.read(cx).content(), "first second"));
    cx.update(|window, cx| {
        let input = view.read(cx).search.find.clone();
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(Some(0..12), "a\n💾", Some(0..4), window, cx);
            assert_eq!(input.content(), "a 💾");
            assert_eq!(input.selected_text_range(false, window, cx).unwrap().range, 0..4);
        });
    });
    cx.run_until_parked();
}

impl bookie_gpui::source::ResultSource for Tiny {
    fn columns(&self) -> &[bookie_gpui::model::ColumnMeta] {
        test_columns()
    }

    fn total(&self) -> u64 {
        3
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        vec![
            Cell::Int(index as i64),
            Cell::Text(format!("tiny {index}")),
            Cell::Bool(index.is_multiple_of(2)),
            Cell::Null,
        ]
    }

    fn find(
        &self,
        query: &str,
        _cap: usize,
        _cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<bookie_gpui::model::FindResult, &'static str> {
        Ok(bookie_gpui::model::FindResult {
            matches: if query == "tiny 2" { vec![2] } else { Vec::new() },
            capped: false,
            scanned: 3,
        })
    }

    fn filter(
        &self,
        _predicate: &bookie_gpui::model::Predicate,
        _cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<Vec<u32>, &'static str> {
        Ok(vec![1])
    }

    fn compare(
        &self,
        keyed: bool,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<bookie_gpui::model::Diff, &'static str> {
        bookie_gpui::source::ResultSource::compare(&bookie_gpui::source::Synthetic::new(3), keyed, cancel)
    }
}

#[gpui::test]
fn the_view_renders_and_searches_whatever_source_it_is_given(cx: &mut TestAppContext) {
    cx.update(text_input::bind_keys);
    let (view, cx) =
        cx.add_window_view(|window, cx| GridView::with_source(std::sync::Arc::new(Tiny), Instant::now(), window, cx));
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.dataset_count(), 3);
        assert_eq!(view.cell_at(1, 1), Cell::Text("tiny 1".into()));
    });
    cx.update(|window, cx| {
        let handle = view.read(cx).search.find.read(cx).focus_handle_for_tests();
        window.focus(&handle, cx);
    });
    cx.simulate_input("tiny 2");
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.search.cursor.matches, vec![2]));
}

#[gpui::test]
fn ctrl_t_opens_one_query_tab_and_ctrl_enter_adds_a_run_with_its_own_result(cx: &mut TestAppContext) {
    use bookie_gpui::shell::Tab;
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    view.read_with(cx, |view, _| {
        assert_eq!(view.tabs.current(), &Tab::Query);
        assert_eq!(view.run_tabs.slots.len(), 1);
        assert_eq!(view.dataset_count(), 150_000);
    });
    cx.simulate_keystrokes("ctrl-enter");
    view.read_with(cx, |view, _| {
        assert_eq!((view.run_tabs.slots.len(), view.run_tabs.active), (2, 1));
        assert_eq!(view.dataset_count(), 1_000_000, "the second run has its own row count");
        assert_eq!(view.pager.page, 0);
    });
    cx.simulate_keystrokes("ctrl-enter ctrl-enter");
    view.read_with(cx, |view, _| assert_eq!(view.dataset_count(), 40));
    cx.simulate_keystrokes("ctrl-t");
    view.read_with(cx, |view, _| {
        assert_eq!(view.tabs.items.len(), 3, "Ctrl+T reuses the query tab")
    });
}

#[gpui::test]
fn selecting_an_earlier_run_restores_its_result_and_clears_the_view_state(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t ctrl-enter down down");
    view.read_with(cx, |view, _| assert!(view.cell_cursor.is_some()));
    cx.update(|_, cx| view.update(cx, |view, _| view.activate_run(0)));
    view.read_with(cx, |view, _| {
        assert_eq!(view.dataset_count(), 150_000);
        assert_eq!(view.cell_cursor, None);
        assert!(view.selection.is_none() && view.cell_edit.edits.is_empty());
        assert!(view.status.contains("Run 1"));
    });
}

#[gpui::test]
fn selecting_the_active_result_keeps_its_filter_and_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.set_mode(crate::Mode::Filtered {
                kept: std::sync::Arc::new(vec![2, 4]),
                label: "filtered".into(),
            });
            view.click_row(1, false);
            view.select_run(view.run_tabs.active);
            assert_eq!(view.dataset_count(), 2);
            assert_eq!(view.selection.map(|selection| selection.focus), Some(1));
        });
    });
}

#[gpui::test]
fn switching_runs_preserves_each_runs_edits(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.cell_edit.edits.insert((1, 1), Cell::Text("first".into()));
            view.add_run(None);
            view.cell_edit.edits.insert((1, 1), Cell::Text("second".into()));
            view.activate_run(0);
            assert_eq!(view.cell_at(1, 1), Cell::Text("first".into()));
            view.activate_run(1);
            assert_eq!(view.cell_at(1, 1), Cell::Text("second".into()));
        })
    });
}

#[gpui::test]
fn switching_grid_tabs_keeps_an_active_cell_editor_open(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.start_edit(0, 1, window, cx);
            view.cell_edit.editor
                .update(cx, |editor, cx| editor.set_content("cell draft", cx));
            view.tabs.open_query();
            view.sync_grid_owner();
        });
    });
    view.read_with(cx, |view, cx| {
        assert!(!view.grid_is_query);
        assert_eq!(
            view.tabs.current(),
            &bookie_gpui::shell::Tab::Table(bookie_gpui::shell::table_info(bookie_gpui::shell::DATA_TABLE))
        );
        assert!(view.cell_edit.active.is_some());
        assert_eq!(view.cell_edit.editor.read(cx).content(), "cell draft");
    });
}

#[gpui::test]
fn switching_results_keeps_an_active_cell_editor_open(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.add_run(None);
            view.add_run(None);
            view.start_edit(0, 1, window, cx);
            view.cell_edit.editor
                .update(cx, |editor, cx| editor.set_content("result draft", cx));
            let active_run = view.run_tabs.active;
            view.select_run(0);
            assert_eq!(view.run_tabs.active, active_run);
        });
    });
    view.read_with(cx, |view, cx| {
        assert!(view.cell_edit.active.is_some());
        assert_eq!(view.cell_edit.editor.read(cx).content(), "result draft");
    });
}

#[gpui::test]
fn closing_runs_releases_edits_and_allows_a_new_run_after_the_last_close(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.cell_edit.edits.insert((1, 1), Cell::Text("first".into()));
            view.add_run(None);
            view.cell_edit.edits.insert((1, 1), Cell::Text("second".into()));
            view.close_run(0);
            assert_eq!(view.run_tabs.slots.len(), 2);
            assert_eq!(view.cell_at(1, 1), Cell::Text("second".into()));
            assert!(view.status.contains("Discard all view-only edits"));
            view.discard_view_edits();
            view.close_run(0);
            assert_eq!(view.run_tabs.slots.len(), 1);
            assert_eq!(view.cell_at(1, 1), Cell::Text("Sample 1".into()));
            view.close_run(0);
            assert!(view.run_tabs.slots.is_empty());
            assert!(view.cell_edit.edits.is_empty());
            assert_eq!(view.dataset_count(), 0);
            view.add_run(None);
            assert_eq!(view.run_tabs.slots[0].run.number, 3);
            assert!(view.cell_edit.edits.is_empty());
        });
    });
}

#[gpui::test]
fn closing_the_final_run_renders_an_empty_result_pane(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.close_run(0);
            cx.notify();
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.run_tabs.slots.is_empty());
        assert_eq!(view.status, "No results");
    });
}

#[gpui::test]
fn closing_the_last_query_result_keeps_the_synthetic_table_available(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.close_run(0);
            assert_eq!(view.dataset_count(), 0);
            view.tabs
                .open_table(&bookie_gpui::shell::table_info(bookie_gpui::shell::DATA_TABLE));
            assert_eq!(view.dataset_count(), 150_000);
        });
    });
}

#[gpui::test]
fn a_new_run_after_closing_all_results_does_not_inherit_table_edits(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.close_run(0);
            view.tabs
                .open_table(&bookie_gpui::shell::table_info(bookie_gpui::shell::DATA_TABLE));
            view.sync_grid_owner();
            view.cell_edit.edits.insert((1, 1), Cell::Text("table edit".into()));
            view.tabs.open_query();
            view.sync_grid_owner();
            view.add_run(None);
            assert_eq!(view.cell_at(1, 1), Cell::Text("Sample 1".into()));
            view.tabs
                .open_table(&bookie_gpui::shell::table_info(bookie_gpui::shell::DATA_TABLE));
            view.sync_grid_owner();
            assert_eq!(view.cell_at(1, 1), Cell::Text("table edit".into()));
        });
    });
}

#[gpui::test]
fn switching_between_table_and_existing_query_keeps_both_edits(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.cell_edit.edits.insert((1, 1), Cell::Text("table".into()));
        })
    });
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert_eq!(view.cell_at(1, 1), Cell::Text("Sample 1".into()));
            view.cell_edit.edits.insert((1, 1), Cell::Text("query".into()));
            view.tabs
                .open_table(&bookie_gpui::shell::table_info(bookie_gpui::shell::DATA_TABLE));
            view.sync_grid_owner();
            assert_eq!(view.cell_at(1, 1), Cell::Text("table".into()));
            view.tabs.open_query();
            view.sync_grid_owner();
            assert_eq!(view.cell_at(1, 1), Cell::Text("query".into()));
        })
    });
}

#[gpui::test]
fn closing_query_tab_releases_its_runs_and_keeps_table_edits(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.cell_edit.edits.insert((1, 1), Cell::Text("table".into()));
        })
    });
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.add_run(None);
            let index = view.tabs.active;
            view.close_tab(index);
            assert!(view.run_tabs.slots.is_empty());
            assert!(view.run_tabs.slots.is_empty());
            assert_eq!(view.cell_at(1, 1), Cell::Text("table".into()));
            view.tabs.open_query();
            view.sync_grid_owner();
            assert_eq!(view.dataset_count(), 0);
            view.add_run(None);
            assert_eq!(view.cell_at(1, 1), Cell::Text("Sample 1".into()));
        })
    });
}

#[gpui::test]
fn closing_an_inactive_query_tab_preserves_the_table(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.cell_edit.edits.insert((1, 1), Cell::Text("table".into()));
        })
    });
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let query = view.tabs.active;
            view.tabs
                .open_table(&bookie_gpui::shell::table_info(bookie_gpui::shell::DATA_TABLE));
            view.sync_grid_owner();
            view.close_tab(query);
            assert!(view.run_tabs.slots.is_empty());
            assert_eq!(view.cell_at(1, 1), Cell::Text("table".into()));
        })
    });
}

#[gpui::test]
fn closing_query_from_home_restores_the_table_owner(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.cell_edit.edits.insert((1, 1), Cell::Text("table".into()));
        })
    });
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let query = view.tabs.active;
            view.tabs.open_home();
            view.sync_grid_owner();
            view.close_tab(query);
            view.tabs
                .open_table(&bookie_gpui::shell::table_info(bookie_gpui::shell::DATA_TABLE));
            view.sync_grid_owner();
            assert!(view.run_tabs.slots.is_empty());
            assert_eq!(view.cell_at(1, 1), Cell::Text("table".into()));
        })
    });
}

#[gpui::test]
fn closing_query_from_home_discards_its_pending_result(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.begin_background(Work::Load, cx, |_| (), |view, ()| view.status = "stale".into());
            let cancel = view.loads.cancel.clone();
            let query = view.tabs.active;
            view.panel.error_location = Some(("SELECT missing".into(), 8, 0, None));
            view.tabs.open_home();
            view.sync_grid_owner();
            assert!(cancel.is_cancelled());
            view.close_tab(query);
            assert!(view.panel.error_location.is_none());
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_ne!(view.status, "stale"));
}

#[gpui::test]
fn the_command_palette_can_close_the_current_result(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t ctrl-shift-p");
    cx.simulate_input("close current result");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert!(view.run_tabs.slots.is_empty()));
}

#[gpui::test]
fn the_command_palette_navigates_between_results(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t ctrl-enter ctrl-shift-p");
    cx.simulate_input("previous result");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert_eq!(view.run_tabs.active, 0));
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("next result");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert_eq!(view.run_tabs.active, 1));
}

#[gpui::test]
fn a_match_in_filtered_rows_uses_its_filtered_position_for_paging(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let kept: Vec<u32> = (0..6_000).map(|row| row * 20).collect();
            view.set_mode(crate::Mode::Filtered {
                kept: std::sync::Arc::new(kept),
                label: "filtered".into(),
            });
            view.search.cursor.set(vec![20_000]);
            view.step_match(true);
            assert_eq!(view.pager.page, 0);
            assert_eq!(view.selection.map(|selection| selection.focus), Some(1_000));
            view.search.cursor.set(vec![110_000]);
            view.step_match(true);
            assert_eq!(view.pager.page, 1);
            assert_eq!(view.selection.map(|selection| selection.focus), Some(5_500));
            view.search.cursor.set(vec![1]);
            view.step_match(true);
            assert_eq!(view.pager.page, 1);
            assert_eq!(view.status, "Match is outside the filtered rows");
        });
    });
}

#[gpui::test]
fn searching_after_filter_selects_the_matching_filtered_row(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let kept: Vec<u32> = (0..6_000).map(|row| row * 20).collect();
            view.set_mode(crate::Mode::Filtered {
                kept: std::sync::Arc::new(kept),
                label: "filtered".into(),
            });
            window.focus(&view.search.find.read(cx).focus_handle_for_tests(), cx);
        });
    });
    cx.simulate_input("Sample 20000");
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.pager.page, 0);
        assert_eq!(view.selection.map(|selection| selection.focus), Some(1_000));
    });
}

#[gpui::test]
fn selecting_a_result_scrolls_its_tab_into_view(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            for _ in 0..12 {
                view.add_run(None);
            }
            cx.notify();
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.run_tabs.scroll.offset().x < gpui::px(0.0)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.activate_run(0);
            cx.notify();
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.run_tabs.scroll.offset().x, gpui::px(0.0)));
}

#[gpui::test]
fn closing_the_active_run_restores_the_surviving_runs_edits(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.add_run(None);
            view.cell_edit.edits.insert((1, 1), Cell::Text("second".into()));
            view.begin_background(Work::Load, cx, |_| (), |view, ()| view.status = "stale result".into());
            view.close_run(1);
            assert_eq!(view.run_tabs.slots.len(), 2);
            assert_eq!(view.cell_at(1, 1), Cell::Text("second".into()));
            assert!(view.status.contains("Discard all view-only edits"));
            view.discard_view_edits();
            view.close_run(1);
            assert_eq!(view.run_tabs.active, 0);
            assert_eq!(view.cell_at(1, 1), Cell::Text("Sample 1".into()));
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_ne!(view.status, "stale result"));
}

#[gpui::test]
fn a_previous_runs_background_result_cannot_replace_the_current_run(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.begin_background(Work::Load, cx, |_| (), |view, ()| view.status = "stale result".into());
            view.add_run(None);
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_ne!(view.status, "stale result"));
}

#[gpui::test]
fn the_results_pane_can_be_hidden_and_the_summary_view_shows_the_run(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("hide the results");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert!(!view.panel.open));
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("show the results");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert!(view.panel.open));
}

fn test_columns() -> &'static [bookie_gpui::model::ColumnMeta] {
    static COLUMNS: std::sync::OnceLock<Vec<bookie_gpui::model::ColumnMeta>> = std::sync::OnceLock::new();
    COLUMNS.get_or_init(bookie_gpui::model::synthetic_columns)
}

struct TwoColumns;

impl bookie_gpui::source::ResultSource for TwoColumns {
    fn columns(&self) -> &[bookie_gpui::model::ColumnMeta] {
        static COLUMNS: std::sync::OnceLock<Vec<bookie_gpui::model::ColumnMeta>> = std::sync::OnceLock::new();
        COLUMNS.get_or_init(|| {
            ["id", "customer_name"]
                .into_iter()
                .map(|name| bookie_gpui::model::ColumnMeta {
                    name: name.into(),
                    width: 150.0,
                    numeric: false,
                    data_type: String::new(),
                    primary_key: false,
                })
                .collect()
        })
    }

    fn total(&self) -> u64 {
        3
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        vec![Cell::Int(index as i64), Cell::Text(format!("customer {index}"))]
    }

    fn find(
        &self,
        _query: &str,
        _cap: usize,
        _cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<bookie_gpui::model::FindResult, &'static str> {
        Err("not needed")
    }

    fn filter(
        &self,
        _predicate: &bookie_gpui::model::Predicate,
        _cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<Vec<u32>, &'static str> {
        Err("not needed")
    }

    fn compare(
        &self,
        _keyed: bool,
        _cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<bookie_gpui::model::Diff, &'static str> {
        Err("not needed")
    }
}

#[gpui::test]
fn a_source_with_other_columns_sets_the_cursor_range_the_labels_and_read_only_editing(cx: &mut TestAppContext) {
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| {
        GridView::with_source(std::sync::Arc::new(TwoColumns), Instant::now(), window, cx)
    });
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("right right right");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_cursor, Some(Cursor { row: 0, column: 1 }));
        assert_eq!(view.status, "Row 1, Customer_name");
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.active.is_none(), "a read-only source refuses edits");
        assert_eq!(view.status, "This cell cannot be edited");
    });
}

#[gpui::test]
fn a_table_cell_selection_sets_the_cursor_and_the_arrows_move_from_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let table = view.read_with(cx, |view, _| view.table.clone());
    cx.update(|_, cx| {
        table.update(cx, |_, cx| {
            cx.emit(gpui_kit::component::table::TableEvent::SelectCell(2, 1))
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_cursor, Some(Cursor { row: 2, column: 1 }))
    });
    cx.simulate_keystrokes("down");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_cursor, Some(Cursor { row: 3, column: 1 }))
    });
}

#[gpui::test]
fn the_settings_tab_opens_and_shows_the_appearance_page(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_settings();
            view.sync_grid_owner();
            cx.notify();
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::Settings)
    });
}

#[gpui::test]
fn settings_font_inputs_keep_editable_values_and_show_defaults(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_settings();
            view.edit_fonts(|fonts| {
                fonts.ui_family.clear();
                fonts.editor_family.clear();
            }, cx);
            cx.notify();
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert!(!view.font_inputs[0].read(cx).placeholder().is_empty());
        assert!(!view.font_inputs[1].read(cx).placeholder().is_empty());
        assert_eq!(view.font_inputs[0].read(cx).content(), "");
    });
    cx.update(|_, cx| {
        let input = view.read(cx).font_inputs[0].clone();
        input.update(cx, |input, cx| input.set_content("Test Font", cx));
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.fonts.settings.ui_family, "Test Font"));
}

#[gpui::test]
fn the_palette_opens_the_history_tab_and_a_status_chip_narrows_the_list(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("open history");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::History)
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.history_page.status = bookie_app_services::statement_history::StatusFilter::Failed;
            cx.notify();
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.history_page.status,
            bookie_app_services::statement_history::StatusFilter::Failed
        )
    });
}

#[gpui::test]
fn the_palette_opens_the_settings_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("open settings");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::Settings)
    });
}

#[gpui::test]
fn a_search_typed_on_one_tab_is_not_left_in_the_find_box_of_the_next(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.search.find.update(cx, |find, cx| find.set_content("Sample 99999", cx))
        })
    });
    cx.run_until_parked();

    cx.simulate_keystrokes("ctrl-t");
    cx.run_until_parked();

    view.read_with(cx, |view, cx| {
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::Query);
        assert_eq!(
            view.search.find.read(cx).content(),
            "",
            "the Query result must not inherit the table's search"
        );
    });
}

#[gpui::test]
fn a_double_click_on_a_table_row_opens_the_active_column_for_editing(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.place_cursor(Some(Cursor { row: 0, column: 1 }));
            view.edit_clicked_row(1, None, window, cx);
        })
    });
    view.read_with(cx, |view, _| {
        let editing = view.cell_edit.active.as_ref().expect("the double-clicked row opens its editor");
        assert_eq!((editing.row, editing.column), (1, 1));
    });
}

