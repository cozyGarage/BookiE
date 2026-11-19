use super::*;
use crate::export::test_support::column;

#[test]
fn csv_header_and_row() {
    let cols = vec![column("a,b")];
    let mut buf = Vec::new();
    write_csv_header(&mut buf, &cols).unwrap();
    write_csv_row(&mut buf, &[Value::Text("hello".into())]).unwrap();
    let s = String::from_utf8(buf).unwrap();
    assert_eq!(s, "\"a,b\"\nhello\n");
}

#[test]
fn rfc4180_csv_header_keeps_formula_like_column_names_unchanged() {
    let columns = vec![column("=expression"), column("+label")];
    let mut output = Vec::new();

    write_csv_header(&mut output, &columns).unwrap();

    assert_eq!(output, b"=expression,+label\n");
}

#[test]
fn the_plain_csv_writers_keep_formula_text_and_render_null_as_an_empty_field() {
    let mut buf = Vec::new();
    write_csv_row(&mut buf, &[Value::Text("=SUM(A1)".into()), Value::Null]).unwrap();
    assert_eq!(String::from_utf8(buf).unwrap(), "=SUM(A1),\n");
}

#[test]
fn csv_writer_uses_explicit_null_marker_without_changing_empty_text() {
    let columns = vec![column("value")];
    let rows = vec![vec![Value::Null], vec![Value::Text(String::new())]];
    let options = CsvOptions {
        null_to_empty: false,
        null_marker: Some("\\N".into()),
        ..CsvOptions::default()
    };

    assert_eq!(render_csv(&columns, &rows, &options), "value\n\\N\n\"\"\n");
}

#[test]
fn csv_null_marker_skips_values_already_in_the_result() {
    let rows = vec![vec![Value::Text("\\N".into())], vec![Value::Text("\\NN".into())]];

    assert_eq!(unique_csv_null_marker(&rows), "\\NNN");
}

#[test]
fn sqlite_any_csv_render_and_column_aware_row_writer_use_type_tags() {
    let columns = [ColumnInfo {
        data_type: "ANY".into(),
        ..crate::export::test_support::column("value")
    }];
    let rows = vec![
        vec![Value::Int(42)],
        vec![Value::Float(1.5)],
        vec![Value::Text("bookie:sqlite-any:v1:integer:9".into())],
        vec![Value::Text(String::new())],
        vec![Value::Null],
        vec![Value::Bytes(vec![0, 255])],
    ];
    let options = CsvOptions {
        null_to_empty: false,
        null_marker: Some("\\N".into()),
        ..CsvOptions::default()
    };
    let rendered = render_csv(&columns, &rows, &options);
    let mut row_writer = Vec::new();
    write_csv_header_with_options(&mut row_writer, &columns, &options).unwrap();
    for row in &rows {
        write_csv_row_with_columns_and_options(&mut row_writer, &columns, row, &options).unwrap();
    }
    assert_eq!(String::from_utf8(row_writer).unwrap(), rendered);
    assert!(rendered.contains("bookie:sqlite-any:v1:integer:42"));
    assert!(rendered.contains("bookie:sqlite-any:v1:blob:00ff"));
    assert!(rendered.contains("bookie:sqlite-any:v1:text:bookie:sqlite-any:v1:integer:9"));
}

#[test]
fn paged_csv_writer_preserves_cell_and_formatting_contracts() {
    let mut any = column("any");
    any.data_type = "ANY".into();
    let mut amount = column("amount");
    amount.data_type = "DECIMAL(8,2)".into();
    let mut payload = column("payload");
    payload.data_type = "BYTEA".into();
    let columns = vec![
        column("id"),
        column("formula"),
        column("nullable"),
        column("empty"),
        amount,
        payload,
        column("json"),
        any,
    ];
    let rows = vec![vec![
        Value::Int(42),
        Value::Text("=SUM(A1)\r\nnext".into()),
        Value::Null,
        Value::Text(String::new()),
        Value::Decimal("12.30".parse().unwrap()),
        Value::Bytes(vec![0, 255]),
        Value::Json(Box::new(serde_json::json!({"ok": true}))),
        Value::Int(7),
    ]];
    let options = CsvOptions {
        null_to_empty: false,
        null_marker: Some("\\N".into()),
        line_break_to_space: true,
        preserve_sqlite_result_types: true,
        delimiter: CsvDelimiter::Semicolon,
        line_break: CsvLineBreak::CrLf,
        decimal: CsvDecimal::Comma,
        ..CsvOptions::default()
    };
    let export = crate::export::ResultExport {
        format: crate::export::ResultFormat::Csv,
        csv: &options,
        sql: None,
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("result.csv");
    let mut pending = Some(rows);

    crate::export::write_paged_file(&path, &columns, 1, &export, || Ok(pending.take()), || false, |_| {}).unwrap();

    assert_eq!(
        std::fs::read(path).unwrap(),
        concat!(
            "id;formula;nullable;empty;amount;payload;json;any\r\n",
            "42;\"'=SUM(A1) next\";\\N;\"\";12,30;\\x00ff;\"{\"\"ok\"\":true}\";",
            "bookie:sqlite-any:v1:integer:7\r\n",
        )
        .as_bytes()
    );
}

#[test]
fn paged_csv_writer_matches_rendered_bytes_across_quote_delimiter_and_line_break_modes() {
    let mut byte_column = column("bytes");
    byte_column.data_type = "BLOB".into();
    let mut any_column = column("any");
    any_column.data_type = "ANY".into();
    let columns = vec![
        column("null"),
        column("empty"),
        column("formula"),
        column("line"),
        column("decimal"),
        byte_column,
        column("json"),
        column("json_text"),
        any_column,
        column("plain"),
    ];
    let rows = vec![vec![
        Value::Null,
        Value::Text(String::new()),
        Value::Text("=1.20".into()),
        Value::Text("\r\n=1.20".into()),
        Value::Text("-12.30".into()),
        Value::Bytes(vec![0, 9, 10, 13, 255]),
        Value::Json(Box::new(serde_json::json!({"x": "a,b"}))),
        Value::JsonText("12.30".into()),
        Value::Int(7),
        Value::Text("a,b|c;d\te".into()),
    ]];
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("result.csv");

    for quote in CsvQuote::ALL {
        for delimiter in CsvDelimiter::ALL {
            for line_break in CsvLineBreak::ALL {
                for spare_plain_numbers in [false, true] {
                    let options = CsvOptions {
                        null_to_empty: false,
                        null_marker: Some("=null\r\n,marker".into()),
                        line_break_to_space: true,
                        preserve_sqlite_result_types: true,
                        spare_plain_numbers,
                        quote,
                        delimiter,
                        line_break,
                        decimal: CsvDecimal::Comma,
                        ..CsvOptions::default()
                    };
                    let expected = render_csv(&columns, &rows, &options);
                    let export = crate::export::ResultExport {
                        format: crate::export::ResultFormat::Csv,
                        csv: &options,
                        sql: None,
                    };
                    let mut pending = Some(rows.clone());
                    crate::export::write_paged_file(
                        &path,
                        &columns,
                        rows.len(),
                        &export,
                        || Ok(pending.take()),
                        || false,
                        |_| {},
                    )
                    .unwrap();

                    assert_eq!(
                        std::fs::read(&path).unwrap(),
                        expected.as_bytes(),
                        "{quote:?} {delimiter:?} {line_break:?} {spare_plain_numbers}"
                    );
                }
            }
        }
    }
}

#[test]
fn sqlite_result_type_tags_are_opt_in_for_fallback_metadata() {
    let columns = [ColumnInfo {
        data_type: "NULL".into(),
        ..crate::export::test_support::column("value")
    }];
    let rows = [vec![Value::Int(42)]];

    assert_eq!(render_csv(&columns, &rows, &CsvOptions::default()), "value\n42\n");
    assert_eq!(
        render_csv(
            &columns,
            &rows,
            &CsvOptions {
                preserve_sqlite_result_types: true,
                ..CsvOptions::default()
            }
        ),
        "value\nbookie:sqlite-any:v1:integer:42\n"
    );
}

#[test]
fn value_contract_csv_keeps_empty_text_distinct_from_null() {
    let columns = vec![column("value")];
    let rows = vec![vec![Value::Null], vec![Value::Text(String::new())]];
    assert_eq!(render_csv(&columns, &rows, &CsvOptions::default()), "value\n\n\"\"\n");
    let always = CsvOptions {
        quote: CsvQuote::Always,
        ..CsvOptions::default()
    };
    assert_eq!(render_csv(&columns, &rows, &always), "\"value\"\n\n\"\"\n");
    let mut buf = Vec::new();
    write_csv_row(&mut buf, &[Value::Null, Value::Text(String::new())]).unwrap();
    assert_eq!(String::from_utf8(buf).unwrap(), ",\"\"\n");
}

#[test]
fn value_contract_csv_round_trip_preserves_bool_uuid_and_json() {
    let mut flag = column("flag");
    flag.data_type = "BOOLEAN".into();
    let mut id = column("id");
    id.data_type = "UUID".into();
    let mut payload = column("payload");
    payload.data_type = "JSONB".into();
    let columns = [flag, id, payload];
    let rows = vec![
        vec![
            Value::Bool(true),
            Value::Uuid("550e8400-e29b-41d4-a716-446655440000".parse().unwrap()),
            Value::Json(Box::new(serde_json::json!({"ok": true, "n": 1}))),
        ],
        vec![
            Value::Bool(false),
            Value::Uuid("00000000-0000-4000-8000-000000000000".parse().unwrap()),
            Value::Json(Box::new(serde_json::json!([null, "a,b", {"x": 2}]))),
        ],
        vec![Value::Null, Value::Null, Value::Null],
    ];
    let csv = render_csv(&columns, &rows, &CsvOptions::default());
    assert_eq!(
        csv,
        concat!(
            "flag,id,payload\n",
            "true,550e8400-e29b-41d4-a716-446655440000,\"{\"\"ok\"\":true,\"\"n\"\":1}\"\n",
            "false,00000000-0000-4000-8000-000000000000,\"[null,\"\"a,b\"\",{\"\"x\"\":2}]\"\n",
            ",,\n",
        )
    );

    let options = crate::import::CsvImportOptions::default();
    let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let restored: Vec<Vec<Value>> = sheet
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            crate::import::row_to_values(row, &[Some(0), Some(1), Some(2)], &columns, &options, index + 2).unwrap()
        })
        .collect();
    assert_eq!(restored, rows);
}

#[test]
fn value_contract_csv_binary_round_trip_preserves_null_empty_and_every_byte() {
    let mut binary_column = column("payload");
    binary_column.data_type = "BYTEA".into();
    let columns = [column("id"), binary_column];
    let rows = vec![
        vec![Value::Text("null".into()), Value::Null],
        vec![Value::Text("empty".into()), Value::Bytes(vec![])],
        vec![Value::Text("binary".into()), Value::Bytes((0..=255).collect())],
    ];
    for delimiter in CsvDelimiter::ALL {
        let csv = render_csv(
            &columns,
            &rows,
            &CsvOptions {
                delimiter,
                ..CsvOptions::default()
            },
        );
        let options = crate::import::CsvImportOptions {
            delimiter,
            ..Default::default()
        };
        let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
        let restored: Vec<Vec<Value>> = sheet
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                crate::import::row_to_values(row, &[Some(0), Some(1)], &columns, &options, index + 2).unwrap()
            })
            .collect();
        assert_eq!(restored, rows);
    }
}

#[test]
fn value_contract_csv_round_trip_preserves_signed_integer_boundaries() {
    let mut value_column = column("value");
    value_column.data_type = "BIGINT".into();
    let columns = [value_column];
    let expected = [i64::MIN, 9_007_199_254_740_993, i64::MAX];
    let rows: Vec<Vec<Value>> = expected.iter().map(|value| vec![Value::Int(*value)]).collect();
    let csv = render_csv(&columns, &rows, &CsvOptions::default());
    assert_eq!(
        csv,
        "value\n-9223372036854775808\n9007199254740993\n9223372036854775807\n"
    );

    let options = crate::import::CsvImportOptions::default();
    let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let imported: Vec<Value> = sheet
        .rows
        .iter()
        .map(|row| crate::import::row_to_values(row, &[Some(0)], &columns, &options, 2).unwrap()[0].clone())
        .collect();
    assert_eq!(imported, expected.into_iter().map(Value::Int).collect::<Vec<_>>());
}

#[test]
fn value_contract_csv_round_trip_preserves_decimal_trailing_zeroes() {
    let mut value_column = column("value");
    value_column.data_type = "DECIMAL(10,4)".into();
    let columns = [value_column];
    let expected: rust_decimal::Decimal = "12.3000".parse().unwrap();
    let csv = render_csv(&columns, &[vec![Value::Decimal(expected)]], &CsvOptions::default());
    assert_eq!(csv, "value\n12.3000\n");

    let options = crate::import::CsvImportOptions::default();
    let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let imported = crate::import::row_to_values(&sheet.rows[0], &[Some(0)], &columns, &options, 2).unwrap();
    let Value::Decimal(actual) = imported[0] else {
        panic!("CSV import did not produce a decimal: {imported:?}");
    };
    assert_eq!(actual.to_string(), "12.3000");
}

#[test]
fn value_contract_csv_round_trip_preserves_negative_zero_bits() {
    let mut value_column = column("value");
    value_column.data_type = "DOUBLE PRECISION".into();
    let columns = [value_column];
    let expected = -0.0f64;
    let csv = render_csv(&columns, &[vec![Value::Float(expected)]], &CsvOptions::default());
    assert_eq!(csv, "value\n-0\n");

    let options = crate::import::CsvImportOptions::default();
    let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let values = crate::import::row_to_values(&sheet.rows[0], &[Some(0)], &columns, &options, 2).unwrap();
    let Value::Float(actual) = values[0] else {
        panic!("CSV import did not produce a float: {values:?}");
    };
    assert_eq!(actual.to_bits(), expected.to_bits());
}

#[test]
fn value_contract_csv_round_trip_preserves_smallest_subnormal_bits() {
    let mut value_column = column("value");
    value_column.data_type = "DOUBLE PRECISION".into();
    let columns = [value_column];
    let expected = f64::from_bits(1);
    let csv = render_csv(&columns, &[vec![Value::Float(expected)]], &CsvOptions::default());

    let options = crate::import::CsvImportOptions::default();
    let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let values = crate::import::row_to_values(&sheet.rows[0], &[Some(0)], &columns, &options, 2).unwrap();
    let Value::Float(actual) = values[0] else {
        panic!("CSV import did not produce a float: {values:?}");
    };
    assert_eq!(actual.to_bits(), expected.to_bits());
}

#[test]
fn value_contract_csv_round_trip_preserves_largest_finite_float_bits() {
    let mut value_column = column("value");
    value_column.data_type = "DOUBLE PRECISION".into();
    let columns = [value_column];
    let expected = f64::MAX;
    let csv = render_csv(&columns, &[vec![Value::Float(expected)]], &CsvOptions::default());

    let options = crate::import::CsvImportOptions::default();
    let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let values = crate::import::row_to_values(&sheet.rows[0], &[Some(0)], &columns, &options, 2).unwrap();
    let Value::Float(actual) = values[0] else {
        panic!("CSV import did not produce a float: {values:?}");
    };
    assert_eq!(actual.to_bits(), expected.to_bits());
}

#[test]
fn value_contract_csv_and_json_round_trip_seeded_finite_float_bits() {
    let mut value_column = column("value");
    value_column.data_type = "DOUBLE PRECISION".into();
    let columns = [value_column];

    // Fixed xorshift seed gives reproducible raw finite-bit samples,
    // including subnormals, without an extra property-test dependency.
    let mut state = 0x6a09_e667_f3bc_c909_u64;
    let mut expected = vec![
        f64::from_bits(0x3fef_ffff_ffff_ffff),
        f64::from_bits(0x3ff0_0000_0000_0001),
        f64::from_bits(0x0000_0000_0000_0002),
        f64::from_bits(0x0010_0000_0000_0001),
        f64::from_bits(0x7fef_ffff_ffff_fffe),
        f64::from_bits(0xffef_ffff_ffff_fffe),
    ];
    for _ in 0..65_536 {
        loop {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            if ((state >> 52) & 0x7ff) != 0x7ff {
                expected.push(f64::from_bits(state));
                break;
            }
        }
    }
    let mantissas = [0, 1, 1 << 51, (1 << 52) - 2, (1 << 52) - 1];
    for sign in [0, 1_u64 << 63] {
        for exponent in 0..0x7ff_u64 {
            for mantissa in mantissas {
                expected.push(f64::from_bits(sign | (exponent << 52) | mantissa));
            }
        }
    }
    assert_eq!(expected.len(), 86_012);
    assert!(expected.iter().all(|value| value.is_finite()));
    let rows = expected
        .iter()
        .map(|value| vec![Value::Float(*value)])
        .collect::<Vec<_>>();

    let csv = render_csv(&columns, &rows, &CsvOptions::default());
    let options = crate::import::CsvImportOptions::default();
    let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let imported = sheet
        .rows
        .iter()
        .map(|row| crate::import::row_to_values(row, &[Some(0)], &columns, &options, 2).unwrap()[0].clone())
        .collect::<Vec<_>>();
    for (expected, actual) in expected.iter().zip(imported) {
        let Value::Float(actual) = actual else {
            panic!("CSV changed a finite float's value type: {actual:?}");
        };
        assert_eq!(actual.to_bits(), expected.to_bits());
    }

    let json: serde_json::Value = serde_json::from_str(&crate::export::render_json(&columns, &rows)).unwrap();
    for (expected, row) in expected.iter().zip(json.as_array().unwrap()) {
        assert_eq!(row["value"].as_f64().unwrap().to_bits(), expected.to_bits());
    }
}

#[test]
fn value_contract_csv_round_trip_preserves_timestamptz_nanoseconds() {
    let mut value_column = column("value");
    value_column.data_type = "TIMESTAMP WITH TIME ZONE".into();
    let columns = [value_column];
    let expected: Value = Value::TimestampTz("2026-09-27T12:34:56.123456789+05:30".parse().unwrap());
    let csv = render_csv(&columns, &[vec![expected.clone()]], &CsvOptions::default());
    assert_eq!(csv, "value\n2026-09-27T07:04:56.123456789+00:00\n");

    let json: serde_json::Value =
        serde_json::from_str(&crate::export::render_json(&columns, &[vec![expected.clone()]])).unwrap();
    assert_eq!(json[0]["value"], "2026-09-27T07:04:56.123456789+00:00");

    let options = crate::import::CsvImportOptions::default();
    let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let values = crate::import::row_to_values(&sheet.rows[0], &[Some(0)], &columns, &options, 2).unwrap();
    assert_eq!(values, vec![expected]);
}

#[test]
fn value_contract_csv_round_trip_keeps_nonfinite_floats_distinct_from_null() {
    let mut value_column = column("value");
    value_column.data_type = "DOUBLE PRECISION".into();
    let columns = [value_column, column("row_tag")];
    let rows = [
        vec![Value::Float(f64::NAN), Value::Text("nan".into())],
        vec![Value::Float(f64::INFINITY), Value::Text("pos".into())],
        vec![Value::Null, Value::Text("null".into())],
        vec![Value::Float(f64::NEG_INFINITY), Value::Text("neg".into())],
    ];
    let csv = render_csv(&columns, &rows, &CsvOptions::default());
    assert_eq!(csv, "value,row_tag\nNaN,nan\ninf,pos\n,null\n-inf,neg\n");

    let options = crate::import::CsvImportOptions::default();
    let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let imported: Vec<Value> = sheet
        .rows
        .iter()
        .map(|row| crate::import::row_to_values(row, &[Some(0), Some(1)], &columns, &options, 2).unwrap()[0].clone())
        .collect();
    assert!(matches!(imported[0], Value::Float(value) if value.is_nan()));
    assert_eq!(imported[1], Value::Float(f64::INFINITY));
    assert_eq!(imported[2], Value::Null);
    assert_eq!(imported[3], Value::Float(f64::NEG_INFINITY));
}

#[test]
fn csv_renderer_applies_export_options_without_losing_cell_content() {
    let columns = vec![column("formula"), column("amount")];
    let rows = vec![vec![
        Value::Text("=SUM(A1:A2)\r\nnext".into()),
        Value::Decimal("12.30".parse().unwrap()),
    ]];
    let options = CsvOptions {
        line_break_to_space: true,
        delimiter: CsvDelimiter::Semicolon,
        quote: CsvQuote::IfNeeded,
        line_break: CsvLineBreak::CrLf,
        decimal: CsvDecimal::Comma,
        ..CsvOptions::default()
    };

    assert_eq!(
        render_csv(&columns, &rows, &options),
        "formula;amount\r\n\"'=SUM(A1:A2) next\";12,30\r\n"
    );
}

#[test]
fn csv_if_needed_quotes_each_structural_and_sanitization_trigger() {
    let columns = vec![
        column("delimiter"),
        column("quote"),
        column("line_break"),
        column("formula"),
    ];
    let rows = vec![vec![
        Value::Text("a,b".into()),
        Value::Text("a\"b".into()),
        Value::Text("line\nbreak".into()),
        Value::Text("=1+1".into()),
    ]];
    let options = CsvOptions {
        line_break_to_space: true,
        ..CsvOptions::default()
    };

    assert_eq!(
        render_csv(&columns, &rows, &options),
        "delimiter,quote,line_break,formula\n\"a,b\",\"a\"\"b\",\"line break\",\"'=1+1\"\n"
    );
}

#[test]
fn csv_decimal_comma_only_rewrites_signed_plain_decimals() {
    let columns = vec![column("value")];
    let values = ["+1.20", "-2.50", "3.40", "1e2", ".5", "5.", "x.5", "5.x"];
    let rows = values
        .iter()
        .map(|value| vec![Value::Text((*value).into())])
        .collect::<Vec<_>>();
    let options = CsvOptions {
        delimiter: CsvDelimiter::Semicolon,
        decimal: CsvDecimal::Comma,
        sanitize_formulas: false,
        ..CsvOptions::default()
    };

    assert_eq!(
        render_csv(&columns, &rows, &options),
        "value\n+1,20\n-2,50\n3,40\n1e2\n.5\n5.\nx.5\n5.x\n"
    );
}

#[test]
fn tsv_renderer_quotes_structural_characters_and_represents_nulls() {
    let columns = vec![column("a\tb"), column("value")];
    let rows = vec![vec![Value::Text("one\ntwo\"three".into()), Value::Null]];

    assert_eq!(
        render_tsv(&columns, &rows, true),
        "\"a\tb\"\tvalue\n\"one\ntwo\"\"three\"\t\\N"
    );
}

#[test]
fn tsv_renderer_keeps_null_empty_marker_and_formula_text_distinct() {
    let columns = vec![column("status")];
    let rows = vec![
        vec![Value::Text("NULL".into())],
        vec![Value::Text(String::new())],
        vec![Value::Null],
        vec![Value::Text("\\N".into())],
        vec![Value::Text("\\NN".into())],
        vec![Value::Text("=1+1".into())],
    ];

    assert_eq!(
        render_tsv(&columns, &rows, false),
        "NULL\n\"\"\n\\NNN\n\"\\N\"\n\"\\NN\"\n\"'=1+1\""
    );
}

#[test]
fn the_formula_guard_can_spare_plain_numbers_but_not_formulas() {
    let guarded = CsvOptions::default();
    let spared = CsvOptions {
        spare_plain_numbers: true,
        ..CsvOptions::default()
    };
    for text in ["-5", "+1.5", "-0.25"] {
        assert_eq!(escape_csv_field(text, &guarded, false), format!("\"'{text}\""));
        assert_eq!(escape_csv_field(text, &spared, false), text);
    }
    for text in ["=1+1", "-5+3", "@SUM(A1)", "+1e5", "-"] {
        assert_eq!(escape_csv_field(text, &spared, false), format!("\"'{text}\""), "{text}");
    }
}

fn custom_options(delimiter: &str) -> CsvOptions {
    CsvOptions {
        custom_delimiter: Some(CustomDelimiter::new(delimiter).unwrap()),
        ..CsvOptions::default()
    }
}

#[test]
fn a_multi_character_delimiter_separates_the_header_and_every_cell() {
    let columns = [column("id"), column("note")];
    let rows = vec![vec![Value::Int(1), Value::Text("plain".into())]];
    let csv = render_csv(&columns, &rows, &custom_options("||"));
    assert_eq!(csv, "id||note\n1||plain\n");
}

#[test]
fn a_cell_holding_the_multi_character_delimiter_is_quoted() {
    let columns = [column("id"), column("note")];
    let rows = vec![vec![Value::Int(1), Value::Text("a||b".into())]];
    let csv = render_csv(&columns, &rows, &custom_options("||"));
    assert_eq!(csv, "id||note\n1||\"a||b\"\n");
}

#[test]
fn an_export_with_a_multi_character_delimiter_is_re_imported_losslessly() {
    let columns = [column("id"), column("note")];
    let rows = vec![
        vec![Value::Int(1), Value::Text("a||b".into())],
        vec![Value::Int(2), Value::Text("say \"hi\"".into())],
        vec![Value::Int(3), Value::Text("two\nlines||x".into())],
        vec![Value::Int(4), Value::Text("::".into())],
    ];
    for delimiter in ["||", "::", "<>", "\u{2192}\u{2192}"] {
        let csv = render_csv(&columns, &rows, &custom_options(delimiter));
        let options = crate::import::CsvImportOptions {
            custom_delimiter: Some(CustomDelimiter::new(delimiter).unwrap()),
            ..Default::default()
        };
        let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
        assert_eq!(sheet.headers, ["id", "note"], "{delimiter}");
        assert_eq!(
            sheet.rows,
            [["1", "a||b"], ["2", "say \"hi\""], ["3", "two\nlines||x"], ["4", "::"]],
            "{delimiter}"
        );
    }
}

#[test]
fn the_single_byte_delimiters_still_ignore_an_unset_custom_delimiter() {
    let columns = [column("a"), column("b")];
    let rows = vec![vec![Value::Int(1), Value::Int(2)]];
    for delimiter in CsvDelimiter::ALL {
        let options = CsvOptions {
            delimiter,
            ..CsvOptions::default()
        };
        assert_eq!(options.delimiter_text(), delimiter.as_str());
        let csv = render_csv(&columns, &rows, &options);
        assert_eq!(csv, format!("a{0}b\n1{0}2\n", delimiter.as_str()));
    }
}

#[test]
fn a_cell_ending_with_part_of_the_delimiter_is_quoted_and_survives() {
    let columns = [column("a"), column("b")];
    for (delimiter, first, second) in [
        ("||", "a|", "b"),
        ("::", ":", ""),
        ("abc", "xab", "y"),
        ("abc", "xa", ""),
    ] {
        let rows = vec![vec![Value::Text(first.into()), Value::Text(second.into())]];
        let csv = render_csv(&columns, &rows, &custom_options(delimiter));
        let options = crate::import::CsvImportOptions {
            custom_delimiter: Some(CustomDelimiter::new(delimiter).unwrap()),
            ..Default::default()
        };
        let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
        assert_eq!(sheet.rows, [[first, second]], "{delimiter:?} {first:?}");
    }
}

#[test]
fn every_short_cell_pair_round_trips_through_each_delimiter() {
    let alphabet = ['|', ':', '"', '\n', 'a'];
    let mut cells = vec![String::new()];
    for length in 1..=2 {
        let mut next = Vec::new();
        for cell in cells.iter().filter(|cell| cell.chars().count() == length - 1) {
            for letter in alphabet {
                next.push(format!("{cell}{letter}"));
            }
        }
        cells.extend(next);
    }
    let columns = [column("a"), column("b")];
    for delimiter in ["||", "::", "|:", "aa"] {
        let options = crate::import::CsvImportOptions {
            custom_delimiter: Some(CustomDelimiter::new(delimiter).unwrap()),
            ..Default::default()
        };
        for first in &cells {
            for second in &cells {
                let rows = vec![vec![Value::Text(first.clone()), Value::Text(second.clone())]];
                let csv = render_csv(&columns, &rows, &custom_options(delimiter));
                let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
                assert_eq!(
                    sheet.rows,
                    [[first.clone(), second.clone()]],
                    "{delimiter:?} {first:?} {second:?}"
                );
            }
        }
    }
}

#[test]
fn a_delimiter_read_from_settings_is_checked_like_one_made_in_code() {
    use serde::de::IntoDeserializer;
    use serde::de::value::{Error, StrDeserializer};
    let read = |text: &'static str| {
        let deserializer: StrDeserializer<'_, Error> = text.into_deserializer();
        <CustomDelimiter as serde::Deserialize>::deserialize(deserializer)
    };
    assert!(read("").is_err());
    assert!(read("a\nb").is_err());
    assert!(read("\"|").is_err());
    assert_eq!(read("||").unwrap().as_str(), "||");
}

#[test]
fn column_names_ending_with_part_of_the_delimiter_survive_in_the_header() {
    let columns = [column("a|"), column("b")];
    let rows = vec![vec![Value::Int(1), Value::Int(2)]];
    let csv = render_csv(&columns, &rows, &custom_options("||"));
    let options = crate::import::CsvImportOptions {
        custom_delimiter: Some(CustomDelimiter::new("||").unwrap()),
        ..Default::default()
    };
    let sheet = crate::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    assert_eq!(sheet.headers, ["a|", "b"]);
    assert_eq!(sheet.rows, [["1", "2"]]);
}

#[test]
fn value_honesty_tsv_copy_keeps_full_binary_booleans_zoned_time_and_all_null_rows() {
    let columns = vec![column("payload"), column("flag"), column("at")];
    let blob: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
    let stamp = Value::TimestampTz("2026-09-27T12:34:56.123456789+05:30".parse().unwrap());
    let rows = vec![
        vec![Value::Bytes(blob.clone()), Value::Bool(true), stamp],
        vec![Value::Null, Value::Null, Value::Null],
    ];
    let expected = format!(
        "\\x{}\ttrue\t2026-09-27T07:04:56.123456789+00:00\n\\N\t\\N\t\\N",
        crate::export::hex_encode(&blob)
    );
    assert_eq!(render_tsv(&columns, &rows, false), expected);
}

#[test]
fn value_honesty_mcp_csv_options_keep_binary_all_null_rows_and_zoned_time() {
    let columns = vec![column("payload"), column("at")];
    let options = CsvOptions {
        null_to_empty: false,
        null_marker: Some("<NULL>".into()),
        sanitize_formulas: false,
        ..CsvOptions::default()
    };
    let blob: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
    let stamp = Value::TimestampTz("2026-09-27T12:34:56+05:30".parse().unwrap());
    let mut output = Vec::new();
    for row in [vec![Value::Bytes(blob.clone()), stamp], vec![Value::Null, Value::Null]] {
        write_csv_row_with_columns_and_options(&mut output, &columns, &row, &options).unwrap();
    }
    assert_eq!(
        String::from_utf8(output).unwrap(),
        format!(
            "\\x{},2026-09-27T07:04:56+00:00\n<NULL>,<NULL>\n",
            crate::export::hex_encode(&blob)
        )
    );
}
