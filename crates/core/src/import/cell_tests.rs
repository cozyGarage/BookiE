use super::*;

#[path = "cell/tests/sqlite_affinity.rs"]
mod sqlite_affinity_tests;

#[path = "cell/tests/column_type_tests.rs"]
mod column_type_tests;

#[path = "cell/tests/temporal_contracts.rs"]
mod temporal_contracts;

fn column(name: &str, data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
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
fn value_contract_parser_preserves_boundaries_and_rejects_rounding() {
    let parse = |kind: ColumnKind| move |text: &str| parse_cell(text, kind).ok();
    crate::parser_contract::assert_numeric_parsers(
        parse(ColumnKind::Int),
        parse(ColumnKind::Decimal),
        parse(ColumnKind::Float),
    );
    for text in crate::parser_contract::corpus_texts("texts") {
        assert_eq!(parse_cell(&text, ColumnKind::Text).unwrap(), Value::Text(text.clone()));
    }
}

#[test]
fn mysql_timestamp_csv_import_restores_the_utc_instant() {
    let options = CsvImportOptions::default();
    let mysql_timestamp = column("instant", "timestamp(6)");
    let expected: Value = Value::TimestampTz("2024-01-02T03:04:05.123456+00:00".parse().unwrap());
    assert_eq!(
        value_for("2024-01-02T03:04:05.123456+00:00", &mysql_timestamp, &options, "mysql"),
        Ok(expected.clone())
    );
    assert_eq!(
        value_for("2024-01-02 03:04:05.123456", &mysql_timestamp, &options, "mysql"),
        Ok(expected),
        "an offset-free MySQL timestamp CSV cell denotes its UTC-session wall time"
    );

    let mysql_datetime = column("local_at", "datetime(6)");
    assert!(value_for("2024-01-02T03:04:05.123456+00:00", &mysql_datetime, &options, "mysql").is_err());
    assert_eq!(
        value_for(
            "2024-01-02 03:04:05.123456",
            &column("local_at", "timestamp without time zone"),
            &options,
            "postgres"
        ),
        Ok(Value::DateTime(
            NaiveDate::from_ymd_opt(2024, 1, 2)
                .unwrap()
                .and_hms_micro_opt(3, 4, 5, 123_456)
                .unwrap()
        )),
        "driver-aware MySQL handling must not change PostgreSQL naive timestamps"
    );
}

#[test]
fn duckdb_interval_restores_only_formula_marked_canonical_text() {
    let options = CsvImportOptions::default();
    let interval = column("span", "INTERVAL");
    assert_eq!(
        value_for("'-1 month -2 days -3 microseconds", &interval, &options, "duckdb"),
        Ok(Value::Text("-1 month -2 days -3 microseconds".into()))
    );
    assert_eq!(
        value_for(
            "'-1 month -2 days -3 microseconds",
            &column("note", "TEXT"),
            &options,
            "duckdb"
        ),
        Ok(Value::Text("'-1 month -2 days -3 microseconds".into()))
    );
    assert_eq!(
        value_for("'-1 month 2 days", &interval, &options, "duckdb"),
        Ok(Value::Text("'-1 month 2 days".into()))
    );
    for malformed in [
        "'-1 months -2 days -3 microseconds",
        "'-1 month -2 day -3 microseconds",
        "'-1 month -2 days -3 microsecond",
    ] {
        assert_eq!(
            value_for(malformed, &interval, &options, "duckdb"),
            Ok(Value::Text(malformed.into())),
            "an invalid unit spelling must not lose its formula marker: {malformed}"
        );
    }
    assert_eq!(value_for("", &interval, &options, "duckdb"), Ok(Value::Null));
    let marked_options = CsvImportOptions {
        null_marker: "NULL".into(),
        ..options
    };
    assert_eq!(
        value_for("", &interval, &marked_options, "duckdb"),
        Err(CellError::NotAnInterval)
    );
}

#[test]
fn postgres_array_csv_cells_remain_text_instead_of_being_parsed_as_scalars() {
    let columns = vec![column("value", "float8[]")];
    let row = vec!["{\"1.0000000000000002\",\"-0\",\"5e-324\",\"NaN\",\"Infinity\",\"-Infinity\",NULL}".into()];
    let values = row_to_values(&row, &[Some(0)], &columns, &CsvImportOptions::default(), 2)
        .expect("float8[] CSV cells remain exact text");

    assert_eq!(values, vec![Value::Text(row[0].clone())]);
}

#[test]
fn value_contract_mssql_datetimeoffset_csv_cells_preserve_the_original_offset_and_scale() {
    let columns = vec![column("zoned", "datetimeoffset(7)")];
    let original = "2024-01-02 03:04:05.1234567 +05:30";
    let row = vec![original.to_owned()];
    let values = row_to_values(&row, &[Some(0)], &columns, &CsvImportOptions::default(), 2).unwrap();

    assert_eq!(values, vec![Value::Text(original.into())]);
}

#[test]
fn value_contract_wide_decimal_csv_cells_remain_exact_text_when_decimal_cannot_hold_them() {
    let columns = vec![column("amount", "decimal(65,30)")];
    let original = "12345678901234567890123456789012345.123456789012345678901234567890";
    let values = row_to_values(
        &[original.to_owned()],
        &[Some(0)],
        &columns,
        &CsvImportOptions::default(),
        2,
    )
    .expect("a valid native DECIMAL value must not be rejected because Rust Decimal is narrower");

    assert_eq!(values, vec![Value::Text(original.into())]);
}

#[test]
fn value_contract_wide_integer_decimal_csv_cells_use_declared_precision_for_text_fallback() {
    let columns = vec![column("amount", "decimal(65,0)")];
    let original = "12345678901234567890123456789012345678901234567890123456789012345";
    let values = row_to_values(
        &[original.to_owned()],
        &[Some(0)],
        &columns,
        &CsvImportOptions::default(),
        2,
    )
    .expect("65 integer digits must remain exact text beyond Rust Decimal's range");

    assert_eq!(values, vec![Value::Text(original.into())]);
}

#[test]
fn declared_decimal_scale_can_equal_precision_when_all_digits_are_fractional() {
    let columns = vec![column("amount", "decimal(30,30)")];
    let original = format!("0.{}", "1".repeat(30));
    let values = row_to_values(
        std::slice::from_ref(&original),
        &[Some(0)],
        &columns,
        &CsvImportOptions::default(),
        2,
    )
    .expect("DECIMAL(30,30) can store thirty fractional digits and no integer digits");

    assert_eq!(values, vec![Value::Text(original)]);
}

#[test]
fn wide_decimal_text_fallback_refuses_values_outside_declared_precision_and_scale() {
    let too_many_integer_digits = format!("{}.{}", "9".repeat(36), "1".repeat(30));
    let too_many_fractional_digits = format!("0.{}", "1".repeat(31));
    for (data_type, value) in [
        ("decimal(65,30)", too_many_integer_digits.as_str()),
        ("decimal(65,30)", too_many_fractional_digits.as_str()),
        (
            "decimal(65,30,1)",
            "12345678901234567890123456789012345.123456789012345678901234567890",
        ),
        ("decimal(30,31)", "0.1234567890123456789012345678901"),
    ] {
        let columns = vec![column("amount", data_type)];
        assert_eq!(
            row_to_values(
                &[value.to_owned()],
                &[Some(0)],
                &columns,
                &CsvImportOptions::default(),
                2,
            )
            .unwrap_err()
            .reason,
            CellError::NotANumber,
            "out-of-range value or malformed precision metadata must be refused for {data_type}"
        );
    }

    assert_eq!(decimal_type_limits("decimal(65,30) garbage"), None);
    assert_eq!(decimal_type_limits("decimal(65,30) unsigned"), Some((65, 30)));
    assert_eq!(decimal_type_limits("decimal(65,30) unsigned zerofill"), Some((65, 30)));
    assert_eq!(decimal_type_limits("decimal(65,30) unsigned garbage"), None);
    assert_eq!(decimal_type_limits("decimal(65,30) unsigned unsigned"), None);
    assert_eq!(decimal_type_limits("decimal(65,30) zerofill zerofill"), None);
}

#[test]
fn wide_decimal_text_fallback_rejects_malformed_numeric_tokens() {
    let columns = vec![column("amount", "decimal(65,30)")];
    for malformed in ["+", ".", "--1", "1e", "1.2.3", "1,000", "'--1"] {
        assert_eq!(
            row_to_values(
                &[malformed.to_owned()],
                &[Some(0)],
                &columns,
                &CsvImportOptions::default(),
                2,
            )
            .unwrap_err()
            .reason,
            CellError::NotANumber,
            "malformed numeric cell must not use the exact-text fallback: {malformed:?}"
        );
    }
}

#[test]
fn formula_safe_wide_decimal_import_removes_only_its_marker() {
    let columns = vec![column("amount", "decimal(65,30)")];
    let value = "-12345678901234567890123456789012345.123456789012345678901234567890";
    let imported = row_to_values(
        &[format!("'{value}")],
        &[Some(0)],
        &columns,
        &CsvImportOptions::default(),
        2,
    )
    .unwrap();

    assert_eq!(imported, vec![Value::Text(value.into())]);
}

#[test]
fn clickhouse_wide_integer_csv_cells_remain_exact_text() {
    let columns = vec![column("signed", "Int128"), column("unsigned", "UInt128")];
    let row = vec![
        "-170141183460469231731687303715884105728".to_owned(),
        "340282366920938463463374607431768211455".to_owned(),
    ];
    let values = row_to_values(&row, &[Some(0), Some(1)], &columns, &CsvImportOptions::default(), 2)
        .expect("wide integer values remain text");

    assert_eq!(values, row.into_iter().map(Value::Text).collect::<Vec<_>>());
}

#[test]
fn formula_marker_is_removed_only_from_valid_signed_wide_integer_cells() {
    let columns = vec![
        column("signed", "Int128"),
        column("unsigned", "UInt128"),
        column("invalid_unsigned", "UInt128"),
        column("overflow", "Int128"),
        column("invalid_signed", "Int128"),
        column("text", "String"),
    ];
    let signed_min = "-170141183460469231731687303715884105728";
    let unsigned_max = "340282366920938463463374607431768211455";
    let row = vec![
        format!("'{signed_min}"),
        format!("'+{unsigned_max}"),
        "'-1".into(),
        "'170141183460469231731687303715884105728".into(),
        "'-not-an-integer".into(),
        "'-170141183460469231731687303715884105728".into(),
    ];
    let values = row_to_values(
        &row,
        &[Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)],
        &columns,
        &CsvImportOptions::default(),
        2,
    )
    .expect("valid wide integer formula prefixes are restored");

    assert_eq!(
        values,
        vec![
            Value::Text(signed_min.into()),
            Value::Text(format!("+{unsigned_max}")),
            Value::Text("'-1".into()),
            Value::Text("'170141183460469231731687303715884105728".into()),
            Value::Text("'-not-an-integer".into()),
            Value::Text("'-170141183460469231731687303715884105728".into()),
        ]
    );
}

#[test]
fn value_contract_formula_safe_decimal_import_rejects_excess_precision() {
    let columns = vec![column("amount", "decimal")];
    for input in ["-0.12345678901234567890123456789", "-1.00000000000000000000000000001"] {
        for text in [input.to_owned(), format!("'{input}")] {
            let error = row_to_values(&[text], &[Some(0)], &columns, &CsvImportOptions::default(), 2)
                .expect_err("excess precision must not be rounded");
            assert_eq!(error.reason, CellError::NotANumber);
            assert_eq!(error.line, 2);
            assert_eq!(error.column, "amount");
        }
    }
}

#[test]
fn formula_marker_is_removed_from_negative_decimal_cells_only_for_decimal_columns() {
    let columns = vec![column("amount", "decimal"), column("text", "String")];
    let row = vec!["'-123.45".to_owned(), "'-123.45".to_owned()];
    let values = row_to_values(&row, &[Some(0), Some(1)], &columns, &CsvImportOptions::default(), 2)
        .expect("sanitized decimal is parsed while text keeps its formula marker");

    assert_eq!(values[0], Value::Decimal(Decimal::new(-12345, 2)));
    assert_eq!(values[1], Value::Text("'-123.45".into()));
}

#[test]
fn value_contract_duckdb_csv_preserves_extended_calendar_values_exactly() {
    let options = CsvImportOptions::default();
    for (data_type, text) in [
        ("DATE", "1000000-02-29"),
        ("DATE", "+10000-01-01"),
        ("DATE", "0001-01-01 (BC)"),
        ("TIMESTAMP", "280000-02-29 12:34:56.123456"),
        ("TIMESTAMP", "+10000-01-01 12:34:56.123456"),
        ("TIMESTAMP", "0001-01-01 (BC) 12:34:56"),
        ("TIMESTAMP WITH TIME ZONE", "280000-02-29 12:34:56.123456+05:30"),
        ("TIMESTAMPTZ", "+10000-01-01 12:34:56.123456+00:00"),
        ("TIMESTAMPTZ", "0001-01-01 (BC) 12:34:56+00"),
    ] {
        let values = row_to_values_for_driver(
            &[text.into()],
            &[Some(0)],
            &[column("value", data_type)],
            &options,
            2,
            "duckdb",
        )
        .expect("valid extended DuckDB calendar value should remain bindable text");
        assert_eq!(values, vec![Value::Text(text.into())]);
    }
    for (data_type, text) in [
        ("DATE", "1000001-02-29"),
        ("TIMESTAMP", "280001-02-29 12:34:56"),
        ("TIMESTAMP", "280000-02-29 12:34:56.123456789"),
        ("TIMESTAMPTZ", "280000-02-29 12:34:56.123456789+00:00"),
        ("TIMESTAMPTZ", "280000-02-29 12:34:56+24:00"),
    ] {
        assert!(
            row_to_values_for_driver(
                &[text.into()],
                &[Some(0)],
                &[column("value", data_type)],
                &options,
                2,
                "duckdb",
            )
            .is_err(),
            "invalid {data_type} input was accepted: {text}"
        );
    }
}

#[test]
fn an_unknown_type_is_read_as_text_rather_than_refused() {
    assert_eq!(column_kind("citext"), ColumnKind::Text);
    assert_eq!(column_kind("my_enum"), ColumnKind::Text);
}

#[test]
fn a_field_that_does_not_fit_its_column_names_the_failure_without_its_contents() {
    let error = parse_cell("not a number", ColumnKind::Int).expect_err("refused");

    assert_eq!(error, CellError::NotAnInteger);
    assert!(!error.to_string().contains("not a number"));
}

#[test]
fn a_row_error_names_the_line_and_column_and_carries_no_cell_text() {
    let columns = vec![column("id", "bigint")];

    let error = row_to_values(
        &["secret-value".to_owned()],
        &[Some(0)],
        &columns,
        &CsvImportOptions::default(),
        4,
    )
    .expect_err("refused");

    assert_eq!(error.line, 4);
    assert_eq!(error.column, "id");
    assert!(!error.to_string().contains("secret-value"));
}

#[test]
fn a_mapped_field_is_parsed_into_the_columns_type() {
    let columns = vec![column("id", "bigint"), column("name", "text")];

    let values = row_to_values(
        &["7".to_owned(), "ada".to_owned()],
        &[Some(0), Some(1)],
        &columns,
        &CsvImportOptions::default(),
        2,
    )
    .expect("values");

    assert_eq!(values, vec![Value::Int(7), Value::Text("ada".to_owned())]);
}

#[test]
fn an_unmapped_column_becomes_null_so_the_server_default_applies() {
    let columns = vec![column("id", "bigint"), column("name", "text")];

    let values = row_to_values(
        &["7".to_owned()],
        &[Some(0), None],
        &columns,
        &CsvImportOptions::default(),
        2,
    )
    .expect("values");

    assert_eq!(values[1], Value::Null);
}

#[test]
fn an_empty_field_is_null_for_a_column_that_is_not_text() {
    let columns = vec![column("id", "bigint")];

    let values =
        row_to_values(&[String::new()], &[Some(0)], &columns, &CsvImportOptions::default(), 2).expect("values");

    assert_eq!(values[0], Value::Null);
}

#[test]
fn an_empty_field_stays_an_empty_string_for_a_text_column() {
    let columns = vec![column("name", "text")];

    let values =
        row_to_values(&[String::new()], &[Some(0)], &columns, &CsvImportOptions::default(), 2).expect("values");

    assert_eq!(values[0], Value::Text(String::new()));
}

#[test]
fn a_null_marker_the_user_set_is_null_even_for_text() {
    let options = CsvImportOptions {
        null_marker: "\\N".to_owned(),
        ..CsvImportOptions::default()
    };
    let columns = vec![column("name", "text")];

    let values = row_to_values(&["\\N".to_owned()], &[Some(0)], &columns, &options, 2).expect("values");

    assert_eq!(values[0], Value::Null);
}

#[test]
fn the_temporal_types_parse_the_forms_an_export_writes() {
    assert!(matches!(parse_cell("2024-05-06", ColumnKind::Date), Ok(Value::Date(_))));
    assert!(matches!(parse_cell("13:45:01", ColumnKind::Time), Ok(Value::Time(_))));
    assert!(matches!(
        parse_cell("2024-05-06 13:45:01", ColumnKind::DateTime),
        Ok(Value::DateTime(_))
    ));
    assert!(matches!(
        parse_cell("2024-05-06T13:45:01Z", ColumnKind::TimestampTz),
        Ok(Value::TimestampTz(_))
    ));
}

#[test]
fn booleans_read_the_spellings_the_engines_export() {
    assert_eq!(parse_cell("TRUE", ColumnKind::Bool), Ok(Value::Bool(true)));
    assert_eq!(parse_cell("f", ColumnKind::Bool), Ok(Value::Bool(false)));
    assert_eq!(parse_cell("maybe", ColumnKind::Bool), Err(CellError::NotABoolean));
}

#[test]
fn binary_takes_hexadecimal_with_or_without_the_engine_prefix() {
    assert_eq!(
        parse_cell("0xDEad", ColumnKind::Bytes),
        Ok(Value::Bytes(vec![0xde, 0xad]))
    );
    assert_eq!(
        parse_cell("0XDEad", ColumnKind::Bytes),
        Ok(Value::Bytes(vec![0xde, 0xad]))
    );
    assert_eq!(
        parse_cell("beef", ColumnKind::Bytes),
        Ok(Value::Bytes(vec![0xbe, 0xef]))
    );
    assert_eq!(parse_cell("xyz", ColumnKind::Bytes), Err(CellError::NotBytes));
    assert_eq!(parse_cell("\\x", ColumnKind::Bytes), Ok(Value::Bytes(vec![])));
    assert_eq!(parse_cell("\\x00ff", ColumnKind::Bytes), Ok(Value::Bytes(vec![0, 255])));
    for text in ["\\x0", "\\xgg", "\\xé", "0x0", "0xgg"] {
        assert_eq!(parse_cell(text, ColumnKind::Bytes), Err(CellError::NotBytes));
    }
}
