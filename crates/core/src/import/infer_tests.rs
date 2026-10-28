use super::*;

fn sheet(headers: &[&str], rows: &[&[&str]]) -> CsvSheet {
    CsvSheet {
        headers: headers.iter().map(|header| (*header).to_owned()).collect(),
        rows: rows
            .iter()
            .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
            .collect(),
        truncated: false,
    }
}

fn inferred(headers: &[&str], rows: &[&[&str]]) -> Vec<DraftColumn> {
    infer_columns(&sheet(headers, rows), &CsvImportOptions::default(), "postgres")
}

#[test]
fn a_field_whose_every_value_is_a_whole_number_is_an_integer() {
    let columns = inferred(&["id"], &[&["1"], &["2"], &["3"]]);

    assert_eq!(columns[0].data_type, "BIGINT");
}

#[test]
fn one_value_that_does_not_fit_widens_the_whole_column_to_text() {
    let columns = inferred(&["id"], &[&["1"], &["2"], &["two"]]);

    assert_eq!(columns[0].data_type, "TEXT");
}

/// `0` and `1` round-trip as both a boolean and a number, and a
/// column of flags-as-numbers read as BOOLEAN would refuse the 2 that
/// turns up later in the file.
#[test]
fn a_field_of_ones_and_zeroes_is_a_number_not_a_boolean() {
    let columns = inferred(&["count"], &[&["0"], &["1"], &["1"]]);

    assert_eq!(columns[0].data_type, "BIGINT");
}

#[test]
fn blank_and_null_marker_cells_do_not_widen_numeric_inference() {
    let options = CsvImportOptions {
        null_marker: "NULL".into(),
        ..Default::default()
    };
    let columns = infer_columns(
        &sheet(&["amount"], &[&["12.30"], &[""], &["NULL"], &["2.50"]]),
        &options,
        "postgres",
    );

    assert_eq!(columns[0].data_type, "NUMERIC");
}

#[test]
fn an_empty_field_is_text_rather_than_a_guess_from_nothing() {
    let columns = inferred(&["note"], &[&[""], &[""]]);

    assert_eq!(columns[0].data_type, "TEXT");
}

#[test]
fn the_narrowest_kind_that_fits_every_value_wins() {
    let columns = inferred(
        &["flag", "amount", "when", "at", "key"],
        &[
            &[
                "true",
                "1.50",
                "2024-05-06",
                "2024-05-06T13:45:01Z",
                "8f14e45f-ceea-467a-9c2b-1f0b2c4d5e6a",
            ],
            &[
                "false",
                "2.25",
                "2024-05-07",
                "2024-05-07T13:45:01Z",
                "9f14e45f-ceea-467a-9c2b-1f0b2c4d5e6b",
            ],
        ],
    );

    let types: Vec<&str> = columns.iter().map(|column| column.data_type.as_str()).collect();
    assert_eq!(types, vec!["BOOLEAN", "NUMERIC", "DATE", "TIMESTAMPTZ", "UUID"]);
}

#[test]
fn only_the_sample_is_read_so_a_long_file_still_opens_at_once() {
    let mut rows: Vec<Vec<String>> = (0..INFER_SAMPLE_ROWS).map(|_| vec!["1".to_owned()]).collect();
    rows.push(vec!["not a number".to_owned()]);
    let sheet = CsvSheet {
        headers: vec!["id".to_owned()],
        rows,
        truncated: false,
    };

    let columns = infer_columns(&sheet, &CsvImportOptions::default(), "postgres");

    assert_eq!(columns[0].data_type, "BIGINT");
}

#[test]
fn every_engine_gets_a_type_name_it_parses() {
    assert_eq!(type_name("sqlite", ColumnKind::DateTime), "TEXT");
    assert_eq!(type_name("mysql", ColumnKind::Uuid), "CHAR(36)");
    assert_eq!(type_name("mssql", ColumnKind::Text), "NVARCHAR(MAX)");
    assert_eq!(type_name("clickhouse", ColumnKind::Int), "BIGINT");
}

#[test]
fn sqlite_import_types_preserve_storage_affinity_for_numeric_and_binary_kinds() {
    for (kind, expected) in [
        (ColumnKind::Bool, "INTEGER"),
        (ColumnKind::Int, "INTEGER"),
        (ColumnKind::Float, "REAL"),
        (ColumnKind::Decimal, "NUMERIC"),
        (ColumnKind::Bytes, "BLOB"),
    ] {
        assert_eq!(type_name("sqlite", kind), expected, "{kind:?}");
    }
}

#[test]
fn a_header_that_is_not_a_usable_name_gets_a_positional_one() {
    let columns = inferred(&["  ", "na\u{7}me"], &[&["1", "2"]]);

    assert_eq!(columns[0].name, "column_1");
    assert_eq!(columns[1].name, "name");
}

#[test]
fn repeated_headers_do_not_produce_two_columns_with_one_name() {
    let columns = inferred(&["id", "ID", "id"], &[&["1", "2", "3"]]);

    let names: Vec<&str> = columns.iter().map(|column| column.name.as_str()).collect();
    assert_eq!(names, vec!["id", "ID_2", "id_3"]);
}

#[test]
fn a_header_longer_than_the_identifier_limit_is_cut_to_it() {
    let long = "x".repeat(MAX_IDENT_BYTES + 50);

    let columns = inferred(&[long.as_str()], &[&["1"]]);

    assert_eq!(columns[0].name.len(), MAX_IDENT_BYTES);
}

#[test]
fn repeated_headers_at_the_identifier_limit_keep_a_unique_suffix() {
    let long = "x".repeat(MAX_IDENT_BYTES);
    let columns = infer_columns(
        &sheet(&[long.as_str(), long.as_str()], &[&["1", "2"]]),
        &CsvImportOptions::default(),
        "postgres",
    );

    assert_eq!(columns[0].name.len(), MAX_IDENT_BYTES);
    assert_eq!(columns[1].name.len(), MAX_IDENT_BYTES);
    assert!(columns[1].name.ends_with("_2"));
    assert_ne!(columns[0].name, columns[1].name);
}

#[test]
fn an_inferred_column_never_constrains_what_the_file_can_hold() {
    let columns = inferred(&["id"], &[&["1"]]);

    assert!(columns[0].nullable);
    assert!(!columns[0].primary_key);
    assert!(!columns[0].auto_increment);
    assert_eq!(columns[0].default_value, None);
}

#[test]
fn only_engines_with_plain_create_table_offer_table_creation_from_a_file() {
    for engine in ["postgres", "mysql", "sqlite", "mssql", "duckdb"] {
        assert!(supports_table_creation(engine), "{engine}");
    }
    for engine in ["clickhouse", "mongodb", "redis", "unknown"] {
        assert!(!supports_table_creation(engine), "{engine}");
    }
}

#[test]
fn duckdb_spells_json_without_the_postgres_binary_suffix() {
    assert_eq!(type_name("duckdb", ColumnKind::Json), "JSON");
    assert_eq!(type_name("postgres", ColumnKind::Json), "JSONB");
    assert_eq!(type_name("duckdb", ColumnKind::Int), "BIGINT");
}

#[test]
fn identifiers_that_look_like_numbers_but_carry_leading_zeros_or_a_plus_stay_text() {
    for values in [
        ["01234", "90210", "00501"],
        ["+15551234567", "+442071234567", "+8613800138000"],
        ["007", "42", "100"],
        ["-01", "5", "6"],
        ["01.5", "2.5", "3.5"],
    ] {
        let rows: Vec<Vec<&str>> = values.iter().map(|value| vec![*value]).collect();
        let rows: Vec<&[&str]> = rows.iter().map(Vec::as_slice).collect();
        assert_eq!(inferred(&["code"], &rows)[0].data_type, "TEXT", "{values:?}");
    }
}

#[test]
fn ordinary_numbers_with_a_single_zero_or_a_leading_minus_stay_numbers() {
    let integers = inferred(&["n"], &[&["0"], &["-5"], &["10"], &["100"]]);
    assert_eq!(integers[0].data_type, "BIGINT");
    let decimals = inferred(&["n"], &[&["0.5"], &["-0.25"], &["10.0"]]);
    assert_ne!(decimals[0].data_type, "TEXT");
}
