use crate::value_input::{
    ParseHooks, ValueInputError, parse_grid_cell_input_with_style, parse_input_for_column,
    parse_input_for_driver_with_style, parse_pasted_cell_with_style,
};
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
fn empty_non_nullable_defaulted_value_is_not_parsed_as_null() {
    let mut timestamp = column("timestamp");
    timestamp.default_value = Some("CURRENT_TIMESTAMP".into());
    assert_eq!(
        parse_input_for_column("", Some(&timestamp)),
        Err(ValueInputError::FieldRequired)
    );
}

#[test]
fn empty_non_nullable_defaulted_text_remains_empty_text() {
    let mut text = column("text");
    text.default_value = Some("'default'".into());
    assert_eq!(parse_input_for_column("", Some(&text)), Ok(Value::Text(String::new())));
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

fn hooks_with(
    style: bookie_core::EnumInputStyle,
    capabilities: bookie_core::DriverValueInputCapabilities,
) -> ParseHooks {
    ParseHooks::new(style, capabilities)
}

fn postgres_hooks() -> ParseHooks {
    hooks_with(
        bookie_core::EnumInputStyle::RawValue,
        bookie_core::DriverValueInputCapabilities {
            special_numeric_literals: true,
            extended_calendar_temporals: true,
            ..bookie_core::DriverValueInputCapabilities::default()
        },
    )
}

fn mysql_hooks() -> ParseHooks {
    hooks_with(
        bookie_core::EnumInputStyle::DeclaredMemberList,
        bookie_core::DriverValueInputCapabilities {
            integer_range_validation: true,
            spatial_edit_requires_native_value: true,
            ..bookie_core::DriverValueInputCapabilities::default()
        },
    )
}

fn duckdb_hooks() -> ParseHooks {
    hooks_with(
        bookie_core::EnumInputStyle::SqlQuotedLabel,
        bookie_core::DriverValueInputCapabilities {
            bit_string_input: true,
            extended_calendar_dates: true,
            timestamp_with_timezone: true,
            temporal_fraction_precision: true,
            ..bookie_core::DriverValueInputCapabilities::default()
        },
    )
}

#[test]
fn duckdb_bit_input_stays_text_for_edit_and_paste() {
    let column = column("BIT");
    for text in ["0", "1", "101010"] {
        assert_eq!(
            parse_input_for_driver_with_style(text, Some(&column), &duckdb_hooks()),
            Ok(Value::Text(text.into()))
        );
        assert_eq!(
            parse_pasted_cell_with_style(Some(text), Some(&column), &duckdb_hooks(), None),
            Ok(Value::Text(text.into()))
        );
    }
    assert_eq!(
        parse_input_for_driver_with_style("1012", Some(&column), &duckdb_hooks()),
        Err(ValueInputError::InvalidBitString)
    );
}

fn sqlite_hooks() -> ParseHooks {
    hooks_with(
        bookie_core::EnumInputStyle::RawValue,
        bookie_core::DriverValueInputCapabilities {
            numeric_affinity_input: true,
            heterogeneous_cell_values: true,
            ..bookie_core::DriverValueInputCapabilities::default()
        },
    )
}

fn nullable(data_type: &str) -> ColumnInfo {
    ColumnInfo {
        nullable: true,
        ..column(data_type)
    }
}

fn edit_text(value: &Value) -> String {
    crate::value_display::format_temporal_for_edit(value)
        .unwrap_or_else(|| crate::value_display::copy_text(value).into_string())
}

#[test]
fn the_edit_text_of_a_time_or_an_instant_parses_back_to_the_same_value_on_every_engine() {
    let time = Value::Time("13:45:30.123456".parse().unwrap());
    let instant = Value::TimestampTz("2024-02-29T08:15:30.123456Z".parse().unwrap());
    let before_epoch = Value::TimestampTz("1969-12-31T23:59:59.5Z".parse().unwrap());
    let cases = [
        (postgres_hooks(), "time", vec![&time]),
        (postgres_hooks(), "timestamptz", vec![&instant, &before_epoch]),
        (mysql_hooks(), "time(6)", vec![&time]),
        (duckdb_hooks(), "TIME", vec![&time]),
        (
            duckdb_hooks(),
            "TIMESTAMP WITH TIME ZONE",
            vec![&instant, &before_epoch],
        ),
        (sqlite_hooks(), "TIME", vec![&time]),
    ];
    for (hooks, data_type, values) in cases {
        for value in values {
            let parsed = parse_grid_cell_input_with_style(
                &edit_text(value),
                Some(&nullable(data_type)),
                &hooks,
                Some(&Value::Null),
            );
            assert_eq!(parsed.as_ref(), Ok(value), "{data_type}");
        }
    }
}

#[test]
fn json_source_text_is_kept_exactly_for_a_json_column_and_parsed_for_jsonb() {
    let documents = [
        r#"{"b":1,"a":2}"#,
        r#"{"a":1,"a":2}"#,
        r#"{"n": 12345678901234567890123}"#,
        "[1, 2.50, 1e400]",
        r#"  {"a" : 1}  "#,
        r#""\u0000""#,
        "null",
    ];
    for (hooks, data_type) in [
        (postgres_hooks(), "json"),
        (duckdb_hooks(), "JSON"),
        (mysql_hooks(), "json"),
    ] {
        for document in documents {
            let expected = Value::JsonText(document.to_owned());
            let column = nullable(data_type);
            assert_eq!(
                parse_grid_cell_input_with_style(document, Some(&column), &hooks, Some(&Value::Null)),
                Ok(expected.clone()),
                "{data_type} {document}"
            );
            assert_eq!(
                parse_input_for_column(document, Some(&column)),
                Ok(expected),
                "{data_type}"
            );
        }
        assert!(matches!(
            parse_input_for_column("{\"a\":", Some(&nullable(data_type))),
            Err(ValueInputError::InvalidJson(_))
        ));
    }
    assert_eq!(
        parse_input_for_column(r#"{"b":1,"a":2}"#, Some(&nullable("jsonb"))),
        Ok(Value::Json(Box::new(serde_json::json!({"b": 1, "a": 2}))))
    );
}

#[test]
fn mysql_year_is_entered_as_the_integer_the_server_reads_as_that_year() {
    let year = nullable("year");
    for text in ["0", "0000"] {
        assert_eq!(
            parse_grid_cell_input_with_style(text, Some(&year), &mysql_hooks(), Some(&Value::Null)),
            Ok(Value::Int(0)),
            "{text}"
        );
        assert_eq!(parse_input_for_column(text, Some(&year)), Ok(Value::Int(0)));
    }
    assert_eq!(
        parse_grid_cell_input_with_style("2024", Some(&year), &mysql_hooks(), Some(&Value::Null)),
        Ok(Value::Int(2024))
    );
}

#[test]
fn a_sqlite_column_with_no_declared_type_keeps_the_storage_class_of_the_cell() {
    let untyped = nullable("");
    let hooks = sqlite_hooks();
    assert_eq!(
        parse_grid_cell_input_with_style("2", Some(&untyped), &hooks, Some(&Value::Int(1))),
        Ok(Value::Int(2))
    );
    assert_eq!(
        parse_grid_cell_input_with_style("2.5", Some(&untyped), &hooks, Some(&Value::Float(1.5))),
        Ok(Value::Float(2.5))
    );
    assert_eq!(
        parse_grid_cell_input_with_style("2", Some(&untyped), &hooks, Some(&Value::Text("1".into()))),
        Ok(Value::Text("2".into()))
    );
    assert_eq!(
        parse_grid_cell_input_with_style("", Some(&untyped), &hooks, Some(&Value::Text("x".into()))),
        Ok(Value::Text(String::new()))
    );
}

#[test]
fn pasting_an_empty_value_into_enum_set_and_bit_varying_stores_the_empty_value_not_null() {
    let mut postgres_enum = nullable("mood");
    postgres_enum.enum_type = Some(bookie_core::QualifiedTypeName {
        schema: "public".into(),
        name: "mood".into(),
    });
    let cases = [
        (postgres_hooks(), postgres_enum),
        (postgres_hooks(), nullable("bit varying")),
        (postgres_hooks(), nullable("varbit")),
        (mysql_hooks(), nullable("enum('a','b c','')")),
        (mysql_hooks(), nullable("set('x','y','z')")),
    ];
    for (hooks, column) in cases {
        assert_eq!(
            parse_pasted_cell_with_style(Some(""), Some(&column), &hooks, Some(&Value::Null)),
            Ok(Value::Text(String::new())),
            "{}",
            column.data_type
        );
        assert_eq!(
            parse_pasted_cell_with_style(None, Some(&column), &hooks, Some(&Value::Null)),
            Ok(Value::Null),
            "{}",
            column.data_type
        );
    }
    assert_eq!(
        parse_pasted_cell_with_style(Some(""), Some(&nullable("enum('a','b')")), &mysql_hooks(), None),
        Err(ValueInputError::UndeclaredEnumMember)
    );
}

#[test]
fn re_entering_the_edit_text_of_an_unchanged_cell_returns_the_cell_unchanged() {
    let cases = [
        (
            postgres_hooks(),
            nullable("json"),
            Value::JsonText(r#"{"a":1,"a":2}"#.into()),
        ),
        (
            postgres_hooks(),
            nullable("numeric"),
            Value::Decimal("1.50".parse().unwrap()),
        ),
        (postgres_hooks(), nullable("double precision"), Value::Float(f64::NAN)),
        (duckdb_hooks(), nullable("JSON"), Value::Text("[1, 2.50, 1e400]".into())),
        (mysql_hooks(), nullable("year"), Value::Int(0)),
        (mysql_hooks(), nullable("year"), Value::Int(2155)),
        (
            mysql_hooks(),
            nullable("json"),
            Value::Json(Box::new(serde_json::json!({"a": 2}))),
        ),
        (sqlite_hooks(), nullable(""), Value::Int(1)),
        (sqlite_hooks(), nullable(""), Value::Float(1.5)),
        (sqlite_hooks(), nullable("NUMERIC"), Value::Int(7)),
        (
            sqlite_hooks(),
            nullable("TIME"),
            Value::Time("00:00:00.5".parse().unwrap()),
        ),
    ];
    for (hooks, column, value) in cases {
        let parsed = parse_grid_cell_input_with_style(&edit_text(&value), Some(&column), &hooks, Some(&value)).unwrap();
        assert!(
            bookie_results::value_exact::exact_equal(&parsed, &value),
            "{}: {value:?} became {parsed:?}",
            column.data_type
        );
    }
}
