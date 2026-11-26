use bookie_gpui::model::Cell;

use super::staged_values;

#[test]
fn a_typed_value_is_parsed_per_cell_and_unchanged_cells_are_left_out() {
    let cells = vec![(0, Cell::Int(1)), (1, Cell::Int(7)), (2, Cell::Int(3))];
    let staged = staged_values(&cells, Some("7")).expect("parses");
    assert_eq!(staged, vec![(0, Cell::Int(7)), (2, Cell::Int(7))]);
}

#[test]
fn a_value_that_does_not_parse_changes_nothing() {
    let cells = vec![(0, Cell::Int(1)), (1, Cell::Int(2))];
    assert!(staged_values(&cells, Some("abc")).is_err());
}

#[test]
fn null_replaces_every_non_null_cell() {
    let cells = vec![(4, Cell::Int(1)), (5, Cell::Null)];
    assert_eq!(staged_values(&cells, None).unwrap(), vec![(4, Cell::Null)]);
}

#[test]
fn an_empty_or_oversized_selection_is_refused() {
    assert!(staged_values(&[], Some("1")).is_err());
    let many: Vec<_> = (0..10_001u64).map(|row| (row, Cell::Int(0))).collect();
    assert!(staged_values(&many, Some("1")).is_err());
}
