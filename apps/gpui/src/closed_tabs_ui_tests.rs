use bookie_core::TableInfo;
use bookie_gpui::shell::Tab;

use super::ClosedTabs;

fn table(name: &str) -> Tab {
    Tab::Table(TableInfo {
        schema: None,
        name: name.into(),
    })
}

#[test]
fn the_most_recently_closed_tab_comes_back_first() {
    let mut closed = ClosedTabs::default();
    closed.remember(table("a"));
    closed.remember(Tab::Query);
    assert_eq!(closed.take(), Some(Tab::Query));
    assert_eq!(closed.take(), Some(table("a")));
    assert_eq!(closed.take(), None);
}

#[test]
fn a_tab_closed_twice_is_remembered_once_and_the_home_tab_never() {
    let mut closed = ClosedTabs::default();
    closed.remember(table("a"));
    closed.remember(table("b"));
    closed.remember(table("a"));
    closed.remember(Tab::Home);
    assert_eq!(closed.take(), Some(table("a")));
    assert_eq!(closed.take(), Some(table("b")));
    assert_eq!(closed.take(), None);
}

#[test]
fn only_the_last_ten_are_kept() {
    let mut closed = ClosedTabs::default();
    for index in 0..15 {
        closed.remember(table(&format!("t{index}")));
    }
    assert_eq!(closed.len(), 10);
    assert_eq!(closed.take(), Some(table("t14")));
}
