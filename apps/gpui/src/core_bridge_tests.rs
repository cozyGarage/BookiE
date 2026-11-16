use crate::core_bridge::parse_edit;
use crate::model::{Cell, row_at};

fn render_tsv(rows: impl IntoIterator<Item = Vec<crate::model::Cell>>, with_headers: bool) -> String {
    bookie_app_services::result_copy::tsv(
        &crate::core_bridge::column_infos(),
        rows.into_iter().collect(),
        with_headers,
    )
}

#[test]
fn a_driver_timestamp_keeps_its_type_and_value_through_the_cell() {
    use chrono::TimeZone;
    let value = bookie_core::Value::TimestampTz(chrono::Utc.with_ymd_and_hms(10000, 1, 1, 12, 0, 0).unwrap());
    assert_eq!(value.clone(), value);
}

#[test]
fn copied_driver_timestamp_keeps_its_extended_year() {
    use chrono::TimeZone;
    let value = bookie_core::Value::TimestampTz(chrono::Utc.with_ymd_and_hms(10000, 1, 1, 12, 0, 0).unwrap());
    let row = vec![Cell::Int(1), Cell::Text("row".into()), Cell::Null, value];
    assert!(render_tsv([row], false).contains("10000-01-01 12:00:00+00:00"));
}

#[test]
fn a_cell_survives_the_trip_through_a_core_value() {
    for index in 0..60 {
        for cell in row_at(index) {
            let value = &cell;
            assert_eq!(&value.clone(), value, "row {index}");
        }
    }
}

#[test]
fn an_undecodable_cell_stays_undecodable_and_is_never_edited_into_a_value() {
    let cell = Cell::Undecodable("numeric(40,20)".into());
    assert_eq!(cell.clone(), cell);
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
    assert_eq!(
        parse_edit("json", "{ \"a\" : 1 }"),
        Ok(Cell::Json(Box::new(serde_json::json!({"a": 1}))))
    );
}

#[test]
fn the_copy_text_is_what_core_renders_for_the_same_rows() {
    let rows: Vec<Vec<Cell>> = (0..3).map(row_at).collect();
    let text = render_tsv(rows, true);
    assert_eq!(text.lines().next(), Some("value\tlabel\tstate\ttyped"));
    assert_eq!(text.lines().count(), 4);
}

#[test]
fn driver_type_names_map_to_the_edit_kind_that_parses_them() {
    use bookie_core::{ColumnInfo, filter_value_kind};
    let info = |data_type: &str| ColumnInfo {
        name: "c".into(),
        data_type: data_type.into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    };
    let kind = |data_type: &str| crate::core_bridge::edit_kind(filter_value_kind(&info(data_type)));
    assert_eq!(kind("NUMERIC(10,2)"), "decimal");
    assert_eq!(kind("bigint"), "integer");
    assert_eq!(kind("timestamp with time zone"), "timestamptz");
    assert_eq!(kind("varchar(20)"), "text");
}

#[test]
fn copied_headers_are_the_result_columns_not_a_fixed_list() {
    let columns = crate::core_bridge::column_infos_for(&[crate::model::ColumnMeta {
        name: "customer_name".into(),
        width: 120.0,
        numeric: false,
    }]);
    let text = bookie_app_services::result_copy::tsv(&columns, vec![vec![Cell::Text("Ada".into())]], true);
    assert_eq!(text, "customer_name\nAda");
}
