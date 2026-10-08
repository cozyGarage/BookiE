use super::*;

#[test]
fn sqlite_numeric_affinity_text_fallback_does_not_apply_to_other_drivers() {
    let options = CsvImportOptions::default();
    for (data_type, expected_error) in [
        ("INTEGER", CellError::NotAnInteger),
        ("REAL", CellError::NotANumber),
        ("NUMERIC", CellError::NotANumber),
    ] {
        let target = column("value", data_type);
        assert_eq!(
            value_for("not numeric", &target, &options, "sqlite"),
            Ok(Value::Text("not numeric".into())),
            "SQLite affinity stores nonnumeric input as text for {data_type}"
        );
        assert_eq!(
            value_for("not numeric", &target, &options, "mysql").unwrap_err(),
            expected_error,
            "the SQLite fallback must not mask a type error for {data_type}"
        );
        assert_eq!(
            value_for("not numeric", &target, &options, "mssql").unwrap_err(),
            expected_error,
            "the SQLite fallback must not mask a type error for {data_type}"
        );
    }
    for (data_type, text, expected_error) in [
        ("INTEGER", "9223372036854775808", CellError::NotAnInteger),
        ("REAL", "1e999", CellError::NotANumber),
        ("REAL", "1e-400", CellError::NotANumber),
        ("NUMERIC", "0.123456789012345678901234567890123", CellError::NotANumber),
    ] {
        assert_eq!(
            value_for(text, &column("value", data_type), &options, "sqlite").unwrap_err(),
            expected_error,
            "numeric-looking overflow, underflow or excess precision must not fall back for {data_type}"
        );
    }
}

#[test]
fn sqlite_unrecognized_numeric_affinity_types_parse_numeric_values_safely() {
    let options = CsvImportOptions::default();
    let target = column("value", "ENUM");
    let strict_any = column("any_value", "ANY");
    assert_eq!(
        value_for("42", &strict_any, &options, "sqlite"),
        Ok(Value::Text("42".into())),
        "without table strictness metadata, ANY keeps its conservative text import"
    );
    assert_eq!(
        value_for("queued", &target, &options, "sqlite"),
        Ok(Value::Text("queued".into()))
    );
    assert_eq!(
        value_for("3.5", &target, &options, "sqlite"),
        Ok(Value::Decimal(Decimal::new(35, 1)))
    );
    for unsafe_number in ["1e999", "1e-400", "0.123456789012345678901234567890123"] {
        assert_eq!(
            value_for(unsafe_number, &target, &options, "sqlite"),
            Err(CellError::NotANumber),
            "unsafe SQLite NUMERIC-affinity input must be refused: {unsafe_number}"
        );
    }
    assert_eq!(
        value_for("3.5", &target, &options, "postgres"),
        Ok(Value::Text("3.5".into()))
    );
}
