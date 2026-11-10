use crate::core_bridge::{cell_to_value, parse_edit, render_tsv, value_to_cell};
use crate::model::{Cell, row_at};

#[test]
fn a_cell_survives_the_trip_through_a_core_value() {
    for index in 0..60 {
        for cell in row_at(index) {
            assert_eq!(value_to_cell(cell_to_value(&cell)), cell, "row {index}");
        }
    }
}

#[test]
fn an_undecodable_cell_stays_undecodable_and_is_never_edited_into_a_value() {
    let cell = Cell::Undecodable("numeric(40,20)".into());
    assert_eq!(value_to_cell(cell_to_value(&cell)), cell);
    assert_eq!(
        parse_edit("undecodable", "1"),
        Err("this cell cannot be edited".to_string())
    );
}

#[test]
fn json_text_keeps_its_exact_bytes_but_json_is_normalised_by_core() {
    assert_eq!(
        parse_edit("json_text", "{\"raw\":  1}"),
        Ok(Cell::JsonText("{\"raw\":  1}".into()))
    );
    assert_eq!(parse_edit("json", "{ \"a\" : 1 }"), Ok(Cell::Json("{\"a\":1}".into())));
}

#[test]
fn the_copy_text_is_what_core_renders_for_the_same_rows() {
    let rows: Vec<Vec<Cell>> = (0..3).map(row_at).collect();
    let text = render_tsv(rows, true);
    assert_eq!(text.lines().next(), Some("value\tlabel\tstate\ttyped"));
    assert_eq!(text.lines().count(), 4);
}
