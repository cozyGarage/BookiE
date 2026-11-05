use std::time::Instant;

use bookie_gpui_grid_prototype::model::Cell;
use gpui::{TestAppContext, VisualTestContext};

use bookie_gpui_grid_prototype::cursor::Cursor;

use crate::GridView;
use crate::text_input;
use crate::ui_theme::ThemeExt;

fn open(cx: &mut TestAppContext) -> (gpui::Entity<GridView>, &mut VisualTestContext) {
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
fn a_valid_edit_is_applied_to_the_typed_cell_and_marked(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(2, 3, window, cx)));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("2026-02-03");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.edits.get(&(2, 3)), Some(&Cell::Date("2026-02-03".into())));
        assert!(view.editing.is_none());
        assert!(view.status.contains("edited in this view only"));
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
        assert!(view.edits.is_empty());
        assert!(view.editing.is_some());
        assert_eq!(view.status, "Not applied: not a date");
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.editing.is_none() && view.edits.is_empty()));
}

#[gpui::test]
fn the_undecodable_cell_cannot_be_edited(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(10, 3, window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.editing.is_none());
        assert_eq!(view.status, "This cell cannot be edited");
    });
}

#[gpui::test]
fn the_find_field_takes_typed_text_and_enter_runs_the_search(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        let handle = view.read(cx).find.read(cx).focus_handle_for_tests();
        window.focus(&handle, cx);
    });
    cx.simulate_input("sample 149999");
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.find.read(cx).content(), "sample 149999");
        assert_eq!(view.cursor.matches, vec![149_999]);
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
        assert_eq!(view.selection.map(|s| s.count()), Some(3));
        assert_eq!(view.cell_cursor, Some(Cursor { row: 4, column: 1 }));
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
        let editing = view.editing.as_ref().expect("the label cell opens its editor");
        assert_eq!((editing.row, editing.column), (1, 1));
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("home enter");
    view.read_with(cx, |view, _| {
        assert!(view.editing.is_none());
        assert_eq!(view.status, "This cell cannot be edited");
    });
}

#[gpui::test]
fn the_palette_filters_runs_and_closes_on_escape(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    view.read_with(cx, |view, _| assert!(view.palette_open));
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
        assert!(!view.palette_open);
        assert_eq!(view.pager.page, 29);
    });
    cx.simulate_keystrokes("ctrl-shift-p escape");
    view.read_with(cx, |view, _| assert!(!view.palette_open));
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
        assert!(!ids.contains(&"clear-filter") && !ids.contains(&"copy") && !ids.contains(&"edit"));
        assert!(ids.contains(&"last"));
    });
    cx.simulate_keystrokes("down");
    view.read_with(cx, |view, _| assert_eq!(view.palette_index, 1));
    cx.simulate_keystrokes("up up");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.palette_index, view.palette_shown(cx).len() - 1)
    });
}

#[gpui::test]
fn the_palette_switches_between_the_light_and_dark_theme(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        assert!(
            !bookie_gpui_grid_prototype::theme::Theme::current(cx).dark,
            "light is the default"
        )
    });
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("dark theme");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(bookie_gpui_grid_prototype::theme::Theme::current(cx).dark));
    view.read_with(cx, |view, _| assert!(view.dark_mode));
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("light theme");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(!bookie_gpui_grid_prototype::theme::Theme::current(cx).dark));
}

#[gpui::test]
fn ctrl_b_hides_and_shows_the_sidebar(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.read_with(cx, |view, _| assert!(view.sidebar_open));
    cx.simulate_keystrokes("ctrl-b");
    view.read_with(cx, |view, _| assert!(!view.sidebar_open));
    cx.simulate_keystrokes("ctrl-b");
    view.read_with(cx, |view, _| assert!(view.sidebar_open));
}

#[gpui::test]
fn the_home_tab_has_no_grid_so_arrow_keys_do_nothing_until_the_table_tab_is_back(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("open home");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert_eq!(view.tabs.active, 0));
    cx.simulate_keystrokes("down down");
    view.read_with(cx, |view, _| assert_eq!(view.cell_cursor, None));
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
        let handle = view.read(cx).sidebar_search.read(cx).focus_handle_for_tests();
        window.focus(&handle, cx);
    });
    cx.simulate_input("aud");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.sidebar_search.read(cx).content(), "aud");
        let rows = bookie_gpui_grid_prototype::shell::tree_rows("aud", true, true);
        assert_eq!(rows.len(), 3, "connection, database and the one matching table");
    });
}

#[gpui::test]
fn the_palette_lists_installed_fonts_and_the_chosen_one_becomes_the_ui_font(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.font_names = vec!["Foo Sans".into(), "Bar Mono".into()]
        })
    });
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("choose font");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, cx| {
        assert!(
            view.palette_open && view.palette_fonts,
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
        assert_eq!(view.font.as_ref(), "Bar Mono");
        assert!(!view.palette_open && !view.palette_fonts);
    });
}

#[gpui::test]
fn the_palette_switches_the_theme_style_and_light_dark_keeps_it(cx: &mut TestAppContext) {
    use bookie_gpui_grid_prototype::theme::Style;
    let (_view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("adwaita");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert_eq!(
            bookie_gpui_grid_prototype::theme::Theme::current(cx).style,
            Style::Adwaita
        )
    });
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("dark theme");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        let theme = bookie_gpui_grid_prototype::theme::Theme::current(cx);
        assert_eq!((theme.style, theme.dark), (Style::Adwaita, true));
    });
}

struct Tiny;

impl bookie_gpui_grid_prototype::source::ResultSource for Tiny {
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

    fn find(&self, query: &str, _cap: usize) -> Result<bookie_gpui_grid_prototype::model::FindResult, &'static str> {
        Ok(bookie_gpui_grid_prototype::model::FindResult {
            matches: if query == "tiny 2" { vec![2] } else { Vec::new() },
            capped: false,
            scanned: 3,
        })
    }

    fn filter(&self, _predicate: &bookie_gpui_grid_prototype::model::Predicate) -> Vec<u32> {
        vec![1]
    }

    fn compare(&self, keyed: bool) -> bookie_gpui_grid_prototype::model::Diff {
        bookie_gpui_grid_prototype::model::build_diff(3, keyed)
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
        let handle = view.read(cx).find.read(cx).focus_handle_for_tests();
        window.focus(&handle, cx);
    });
    cx.simulate_input("tiny 2");
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.cursor.matches, vec![2]));
}

#[gpui::test]
fn ctrl_t_opens_one_query_tab_and_ctrl_enter_adds_a_run_with_its_own_result(cx: &mut TestAppContext) {
    use bookie_gpui_grid_prototype::shell::Tab;
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    view.read_with(cx, |view, _| {
        assert_eq!(view.tabs.current(), &Tab::Query);
        assert_eq!(view.runs.len(), 1);
        assert_eq!(view.dataset_count(), 150_000);
    });
    cx.simulate_keystrokes("ctrl-enter");
    view.read_with(cx, |view, _| {
        assert_eq!((view.runs.len(), view.active_run), (2, 1));
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
        assert!(view.selection.is_none() && view.edits.is_empty());
        assert!(view.status.contains("Run 1"));
    });
}

#[gpui::test]
fn the_results_pane_can_be_hidden_and_the_summary_view_shows_the_run(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("hide the results");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert!(!view.result_open));
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("show the results");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert!(view.result_open));
}
