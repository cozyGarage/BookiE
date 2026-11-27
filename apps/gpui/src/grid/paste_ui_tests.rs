use bookie_gpui::model::{Cell, CellText};

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

#[test]
fn an_unchanged_cell_pasted_over_itself_is_left_out_whatever_its_kind() {
    let unchanged = [
        Cell::Float(f64::NAN),
        Cell::Time("13:45:30.5".parse().unwrap()),
        Cell::TimestampTz("2024-02-29T08:15:30.123456Z".parse().unwrap()),
        Cell::JsonText(r#"{"a":1,"a":2}"#.into()),
        Cell::Decimal("1.50".parse().unwrap()),
    ];
    let targets: Vec<PasteTarget> = unchanged
        .iter()
        .enumerate()
        .map(|(row, cell)| target(row as u64, 1, Some(&cell.display()), cell.clone()))
        .collect();
    assert_eq!(staged_paste(&targets).unwrap(), vec![]);
}

#[test]
fn a_value_that_only_compares_equal_numerically_is_still_a_change() {
    let staged = staged_paste(&[
        target(0, 1, Some("-0"), Cell::Float(0.0)),
        target(1, 1, Some("1.500"), Cell::Decimal("1.50".parse().unwrap())),
    ])
    .unwrap();
    assert_eq!(staged.len(), 2, "{staged:?}");
}
