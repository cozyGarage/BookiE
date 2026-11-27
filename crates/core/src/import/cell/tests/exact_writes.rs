use super::*;

#[test]
fn a_json_column_imports_the_source_text_exactly_and_jsonb_imports_the_parsed_document() {
    let options = CsvImportOptions::default();
    for document in [
        r#"{"b":1,"a":2}"#,
        r#"{"a":1,"a":2}"#,
        "[1, 2.50, 1e400]",
        r#"  {"a" : 1}  "#,
    ] {
        for (driver_id, data_type) in [("postgres", "json"), ("duckdb", "JSON"), ("mysql", "json")] {
            assert_eq!(
                value_for(document, &column("value", data_type), &options, driver_id),
                Ok(Value::JsonText(document.to_owned())),
                "{driver_id} {document}"
            );
        }
    }
    assert_eq!(
        value_for("{\"a\":", &column("value", "json"), &options, "postgres"),
        Err(CellError::NotJson)
    );
    assert_eq!(
        value_for(r#"{"b":1,"a":2}"#, &column("value", "jsonb"), &options, "postgres"),
        Ok(Value::Json(Box::new(serde_json::json!({"b": 1, "a": 2}))))
    );
}

#[test]
fn a_mysql_year_imports_as_the_integer_the_server_reads_as_that_year() {
    let options = CsvImportOptions::default();
    let year = column("value", "year");
    for text in ["0", "0000"] {
        assert_eq!(value_for(text, &year, &options, "mysql"), Ok(Value::Int(0)), "{text}");
    }
    assert_eq!(value_for("1901", &year, &options, "mysql"), Ok(Value::Int(1901)));
    assert_eq!(
        value_for("next", &year, &options, "mysql"),
        Err(CellError::NotAnInteger)
    );
}
