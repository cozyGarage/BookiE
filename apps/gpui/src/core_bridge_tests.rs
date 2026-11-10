use crate::core_bridge::{cell_to_value, parse_edit, render_tsv, value_to_cell};
use crate::model::{Cell, row_at};

#[test]
fn a_driver_timestamp_keeps_its_type_and_value_through_the_cell() {
    use chrono::TimeZone;
    let value = bookie_core::Value::TimestampTz(chrono::Utc.with_ymd_and_hms(10000, 1, 1, 12, 0, 0).unwrap());
    assert_eq!(cell_to_value(&value_to_cell(value.clone())), value);
}

#[test]
fn copied_driver_timestamp_keeps_its_extended_year() {
    use chrono::TimeZone;
    let value = bookie_core::Value::TimestampTz(chrono::Utc.with_ymd_and_hms(10000, 1, 1, 12, 0, 0).unwrap());
    let row = vec![Cell::Int(1), Cell::Text("row".into()), Cell::Null, value_to_cell(value)];
    assert!(render_tsv([row], false).contains("+10000-01-01T12:00:00+00:00"));
}

#[test]
fn a_cell_survives_the_trip_through_a_core_value() {
    for index in 0..60 {
        for cell in row_at(index) {
            let value = cell_to_value(&cell);
            assert_eq!(cell_to_value(&value_to_cell(value.clone())), value, "row {index}");
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
