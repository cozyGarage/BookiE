use gpui::{Modifiers, MouseButton, TestAppContext};

use crate::GridView;
use crate::live_ui_tests::open_live;
use crate::save_ui_tests::{fixture_copy, saving_view, settle_saving};
use crate::table_menu::TableMenuItem;
use bookie_core::TableInfo;
use bookie_gpui::shell::{SidebarFilter, Tab, TreeRow, tree_rows_grouped};

fn customers() -> TableInfo {
    TableInfo {
        schema: None,
        name: "customers".into(),
    }
}

fn tree_index(view: &GridView, table: &TableInfo) -> usize {
    tree_rows_grouped(
        &view.catalog,
        "",
        true,
        true,
        SidebarFilter::default(),
        &view.sidebar.collapsed_schemas,
    )
    .iter()
    .position(|(_, row)| matches!(row, TreeRow::Table(found) if found == table))
    .unwrap_or_else(|| panic!("{} is not in the sidebar", table.name))
}

fn right_click_table(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, table: &TableInfo) {
    let index = view.read_with(cx, |view, _| tree_index(view, table));
    let selector: &'static str = Box::leak(format!("sidebar-row-{index}").into_boxed_str());
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{} is not drawn", table.name));
    cx.simulate_mouse_down(bounds.center(), MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn right_clicking_a_sidebar_table_offers_its_actions_with_destructive_ones_last(cx: &mut TestAppContext) {
    let path = fixture_copy("table-menu");
    let (view, cx) = saving_view(cx, &path);
    right_click_table(&view, cx, &customers());
    view.read_with(cx, |view, _| {
        assert!(view.row_menu.is_some(), "the table menu is open");
        assert_eq!(
            view.table_menu_items(),
            vec![
                vec![
                    TableMenuItem::OpenData,
                    TableMenuItem::Structure,
                    TableMenuItem::CreateScript,
                    TableMenuItem::SelectQuery,
                ],
                vec![TableMenuItem::CopyName, TableMenuItem::Export],
                vec![TableMenuItem::Empty, TableMenuItem::Drop],
            ]
        );
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn open_data_from_the_table_menu_opens_that_table(cx: &mut TestAppContext) {
    let path = fixture_copy("table-menu-open");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_table_menu_item(TableMenuItem::OpenData, &customers(), window, cx)
        })
    });
    for _ in 0..100 {
        settle_saving(cx);
        if view.read_with(cx, |view, _| view.catalog.grid_table == customers()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    view.read_with(cx, |view, _| {
        assert_eq!(view.catalog.grid_table, customers(), "{}", view.status);
        assert_eq!(view.tabs.current(), &Tab::Table(customers()));
        assert!(view.row_menu.is_none());
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_read_only_connection_hides_empty_and_drop(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    right_click_table(&view, cx, &customers());
    view.read_with(cx, |view, _| {
        assert!(view.row_menu.is_some(), "the table menu is open");
        let items = view.table_menu_items().concat();
        assert!(items.contains(&TableMenuItem::OpenData));
        assert!(!items.contains(&TableMenuItem::Empty));
        assert!(!items.contains(&TableMenuItem::Drop));
    });
}

#[gpui::test]
fn new_query_from_the_table_menu_prefills_a_quoted_select(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_table_menu_item(TableMenuItem::SelectQuery, &customers(), window, cx)
        })
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(view.tabs.current(), &Tab::Query);
        assert_eq!(view.sql_input.read(cx).value().as_ref(), "SELECT * FROM \"customers\";");
    });
}
