use crate::value_input::{ParseHooks, ValueInputError, parse_input_for_column, parse_input_for_driver_with_style};
use bookie_core::{ColumnInfo, Value};

fn column(data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: "value".into(),
        data_type: data_type.into(),
        nullable: false,
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
fn invalid_integer_returns_a_typed_error_without_localized_text() {
    let number = column("integer");
    assert_eq!(
        parse_input_for_column("not-a-number", Some(&number)),
        Err(ValueInputError::InvalidInteger)
    );
    assert_eq!(
        parse_input_for_driver_with_style(
            "not-a-number",
            Some(&number),
            &ParseHooks::without_driver(bookie_core::EnumInputStyle::RawValue),
        ),
        Err(ValueInputError::InvalidInteger)
    );
}

#[test]
fn mysql_range_failure_returns_an_engine_specific_error() {
    let hooks = ParseHooks::new(
        bookie_core::EnumInputStyle::RawValue,
        bookie_core::DriverValueInputCapabilities {
            integer_range_validation: true,
            ..bookie_core::DriverValueInputCapabilities::default()
        },
    );
    assert_eq!(
        parse_input_for_driver_with_style("4294967296", Some(&column("int unsigned")), &hooks),
        Err(ValueInputError::MySqlIntegerOutOfRange)
    );
}

#[test]
fn sqlite_affinity_is_selected_by_capability_and_preserves_numeric_text() {
    let numeric_column = column("NUMERIC");
    let hooks = ParseHooks::new(
        bookie_core::EnumInputStyle::RawValue,
        bookie_core::DriverValueInputCapabilities {
            numeric_affinity_input: true,
            ..bookie_core::DriverValueInputCapabilities::default()
        },
    );
    assert_eq!(
        parse_input_for_driver_with_style("9e2", Some(&numeric_column), &hooks),
        Ok(Value::Decimal("9e2".parse().unwrap()))
    );
    assert!(
        parse_input_for_driver_with_style(
            "9e2",
            Some(&column("integer")),
            &ParseHooks::without_driver(bookie_core::EnumInputStyle::RawValue),
        )
        .is_err()
    );
}
