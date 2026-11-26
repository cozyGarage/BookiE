use bookie_gpui::model::Cell;

use super::{PasteTarget, staged_paste};

fn target(row: u64, column: usize, text: Option<&str>, current: Cell) -> PasteTarget {
    PasteTarget {
        row,
        column,
        text: text.map(str::to_string),
        current,
    }
}

#[test]
fn each_pasted_cell_is_parsed_with_the_kind_of_the_cell_it_replaces() {
    let staged = staged_paste(&[
        target(0, 1, Some("hello"), Cell::Text("old".into())),
        target(0, 2, Some("42"), Cell::Int(1)),
    ])
    .expect("parses");
    assert_eq!(
        staged,
        vec![((0, 1), Cell::Text("hello".into())), ((0, 2), Cell::Int(42))]
    );
}

#[test]
fn a_missing_text_pastes_null_and_unchanged_cells_are_left_out() {
    let staged = staged_paste(&[
        target(3, 1, None, Cell::Text("x".into())),
        target(4, 1, Some("same"), Cell::Text("same".into())),
    ])
    .unwrap();
    assert_eq!(staged, vec![((3, 1), Cell::Null)]);
}

#[test]
fn one_cell_that_does_not_parse_changes_nothing() {
    let error = staged_paste(&[
        target(0, 1, Some("fine"), Cell::Text("a".into())),
        target(1, 2, Some("not a number"), Cell::Int(1)),
    ])
    .unwrap_err();
    assert!(error.contains("row 2"), "{error}");
}
