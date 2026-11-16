use super::parse_input_for_grid_cell;
use bookie_core::{ColumnInfo, Value};

fn nullable_column(data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: "label".into(),
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
    }
}

#[test]
fn confirming_an_empty_string_cell_keeps_the_empty_string_and_does_not_write_null() {
    let empty = Value::Text(String::new());
    for (driver, data_type) in [
        ("mssql", "nvarchar"),
        ("mssql", "varchar"),
        ("mssql", "nvarchar(max)"),
        ("mssql", "text"),
        ("mysql", "varchar(10)"),
        ("postgres", "text"),
    ] {
        let column = nullable_column(data_type);
        assert_eq!(
            parse_input_for_grid_cell("", Some(&column), driver, Some(&empty)).unwrap(),
            empty,
            "{driver} {data_type}: an empty string must stay an empty string"
        );
    }
}

#[test]
fn confirming_a_null_cell_keeps_null() {
    let column = nullable_column("nvarchar");
    assert_eq!(
        parse_input_for_grid_cell("", Some(&column), "mssql", Some(&Value::Null)).unwrap(),
        Value::Null
    );
}

#[test]
fn an_empty_binary_cell_is_not_turned_into_text_or_null_by_a_text_edit() {
    let column = nullable_column("varbinary(max)");
    let empty = Value::Bytes(Vec::new());
    assert_eq!(
        parse_input_for_grid_cell("", Some(&column), "mssql", Some(&empty)).unwrap(),
        empty,
        "an empty varbinary value must stay empty bytes"
    );
}
