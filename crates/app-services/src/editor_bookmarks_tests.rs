use super::*;

#[test]
fn toggling_a_line_adds_then_removes_it() {
    let mut bookmarks = BookmarkSet::new();
    assert!(bookmarks.toggle(3));
    assert!(bookmarks.contains(3));
    assert!(!bookmarks.toggle(3));
    assert!(!bookmarks.contains(3));
    assert!(bookmarks.is_empty());
}

#[test]
fn adjacent_finds_the_next_and_previous_without_wrapping() {
    let bookmarks = BookmarkSet::from_lines([1, 4, 9]);
    assert_eq!(bookmarks.adjacent(1, true), Some(4));
    assert_eq!(bookmarks.adjacent(4, true), Some(9));
    assert_eq!(bookmarks.adjacent(9, true), None);
    assert_eq!(bookmarks.adjacent(9, false), Some(4));
    assert_eq!(bookmarks.adjacent(4, false), Some(1));
    assert_eq!(bookmarks.adjacent(1, false), None);
    assert_eq!(bookmarks.adjacent(5, true), Some(9));
    assert_eq!(bookmarks.adjacent(5, false), Some(4));
}

#[test]
fn clearing_removes_every_bookmark() {
    let mut bookmarks = BookmarkSet::from_lines([0, 2, 7]);
    bookmarks.clear();
    assert!(bookmarks.is_empty());
    assert_eq!(bookmarks.adjacent(0, true), None);
}

#[test]
fn duplicate_lines_collapse_to_one_entry() {
    let bookmarks = BookmarkSet::from_lines([2, 2, 5, 2]);
    assert_eq!(bookmarks.lines().collect::<Vec<_>>(), vec![2, 5]);
}

#[test]
fn an_empty_set_has_no_adjacent_target() {
    let bookmarks = BookmarkSet::new();
    assert_eq!(bookmarks.adjacent(0, true), None);
    assert_eq!(bookmarks.adjacent(10, false), None);
}
