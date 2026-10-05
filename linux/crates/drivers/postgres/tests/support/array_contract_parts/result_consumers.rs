#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_text_array_file_exports_preserve_text_and_escape_markup() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression =
        r#"ARRAY['NULL', NULL, '', 'comma,value', E'quote"slash\\', '東京 😀', '</value><injected a="1">&']::text[]"#;
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("text[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, {expression}::text, \
                    array_to_json({expression})::text, encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("text[]".into()));
    assert_eq!(result.rows[0][0], oracle.rows[0][1]);
    let round_trip = connection
        .query_params(
            "SELECT encode(array_send($1::text[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(round_trip.rows[0][0], oracle.rows[0][3]);
    let Value::Text(native_json) = &oracle.rows[0][2] else {
        panic!("native array_to_json returned {:?}", oracle.rows[0][2]);
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(native_json).unwrap(),
        serde_json::json!([
            "NULL",
            null,
            "",
            "comma,value",
            "quote\"slash\\",
            "東京 😀",
            "</value><injected a=\"1\">&"
        ])
    );

    let directory = tempfile::tempdir().unwrap();
    let options = tablepro_core::export::CsvOptions::default();
    for (extension, format, marker) in [
        (
            "xml",
            tablepro_core::export::ResultFormat::Xml,
            "&lt;/value&gt;&lt;injected",
        ),
        (
            "html",
            tablepro_core::export::ResultFormat::Html,
            "&lt;/value&gt;&lt;injected",
        ),
        (
            "md",
            tablepro_core::export::ResultFormat::Markdown,
            "&lt;/value&gt;&lt;injected",
        ),
    ] {
        let path = directory.path().join(format!("array.{extension}"));
        tablepro_core::export::write_result_file(
            &path,
            &result,
            &tablepro_core::export::ResultExport {
                format,
                csv: &options,
                sql: None,
            },
            || false,
            |_| {},
        )
        .unwrap();
        let output = std::fs::read_to_string(path).unwrap();
        assert!(output.contains(marker), "{extension}: {output}");
        assert!(!output.contains("</value><injected"), "{extension}: {output}");
        assert!(output.contains("NULL"), "{extension}: {output}");
        assert!(output.contains("comma,value"), "{extension}: {output}");
        assert!(output.contains("quote"), "{extension}: {output}");
        assert!(output.contains("slash"), "{extension}: {output}");
        assert!(output.contains("東京 😀"), "{extension}: {output}");
    }

    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("array.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(
        shared_strings.contains("\"NULL\",NULL,\"\",\"comma,value\""),
        "{shared_strings}"
    );
    assert!(shared_strings.contains("quote"), "{shared_strings}");
    assert!(shared_strings.contains("東京 😀"), "{shared_strings}");
    assert!(
        shared_strings.contains("&lt;/value&gt;&lt;injected"),
        "{shared_strings}"
    );
    assert!(!sheet.contains("<f>"), "{sheet}");

    connection
        .execute("CREATE TABLE array_filewriter_target (value text[])")
        .await
        .unwrap();
    let path = directory.path().join("array.sql");
    tablepro_core::export::write_result_file(
        &path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Sql,
            csv: &options,
            sql: Some(tablepro_core::export::SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "array_filewriter_target",
            }),
        },
        || false,
        |_| {},
    )
    .unwrap();
    connection
        .execute(&std::fs::read_to_string(path).unwrap())
        .await
        .unwrap();
    let restored = connection
        .query(
            "SELECT value::text, array_to_json(value)::text, \
                    encode(array_send(value), 'hex') FROM array_filewriter_target",
        )
        .await
        .unwrap();
    assert_eq!(restored.rows, vec![oracle.rows[0][1..].to_vec()]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_uuid_array_file_exports_preserve_values_for_calc_reimport() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY[\
        '550e8400-e29b-41d4-a716-446655440000'::uuid, \
        '12345678-1234-5678-90ab-1234567890ab'::uuid, NULL]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("uuid[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, {expression}::text, \
             array_to_json({expression})::text, encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("uuid[]".into()));
    assert_eq!(
        oracle.rows[0][2],
        Value::Text(
            r#"["550e8400-e29b-41d4-a716-446655440000","12345678-1234-5678-90ab-1234567890ab",null]"#.into()
        )
    );
    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::uuid[])::text, \
             encode(array_send($1::text::uuid[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][2]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][3]);

    let directory = tempfile::tempdir().unwrap();
    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("uuid-array.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(),
        &mut sheet,
    )
    .unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("uuid[] result must remain text: {:?}", result.rows[0][0]);
    };
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_date_array_file_exports_preserve_boundaries_for_calc_reimport() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY[\
        '0002-12-31 BC'::date, '10000-01-01'::date, \
        'infinity'::date, '-infinity'::date, NULL]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("date[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, array_to_json({expression})::text, \
             encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("date[]".into()));
    assert_eq!(
        oracle.rows[0][1],
        Value::Text(
            r#"["0002-12-31 BC","10000-01-01","infinity","-infinity",null]"#.into()
        )
    );
    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::date[])::text, \
             encode(array_send($1::text::date[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][1]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][2]);

    let directory = tempfile::tempdir().unwrap();
    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("date-array.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(),
        &mut sheet,
    )
    .unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("date[] result must remain text: {:?}", result.rows[0][0]);
    };
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_timestamp_array_file_exports_preserve_boundaries_for_calc_reimport() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY[\
        '0002-12-31 23:59:59.999999 BC'::timestamp, \
        '10000-01-01 00:00:00'::timestamp, \
        '294276-12-31 23:59:59.999999'::timestamp, \
        'infinity'::timestamp, '-infinity'::timestamp, NULL]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("timestamp[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, array_to_json({expression})::text, \
             encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(
        oracle.rows[0][0],
        Value::Text("timestamp without time zone[]".into())
    );
    assert_eq!(
        oracle.rows[0][1],
        Value::Text(
            r#"["0002-12-31T23:59:59.999999 BC","10000-01-01T00:00:00","294276-12-31T23:59:59.999999","infinity","-infinity",null]"#.into()
        )
    );
    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::timestamp[])::text, \
             encode(array_send($1::text::timestamp[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][1]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][2]);

    let directory = tempfile::tempdir().unwrap();
    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("timestamp-array.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(),
        &mut sheet,
    )
    .unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("timestamp[] result must remain text: {:?}", result.rows[0][0]);
    };
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_time_array_file_exports_preserve_boundaries_for_calc_reimport() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY[\
        '00:00:00'::time, '12:34:56.123456'::time, \
        '23:59:59.999999'::time, '24:00:00'::time, NULL]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("time[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, array_to_json({expression})::text, \
             encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(
        oracle.rows[0][0],
        Value::Text("time without time zone[]".into())
    );
    assert_eq!(
        oracle.rows[0][1],
        Value::Text(
            r#"["00:00:00","12:34:56.123456","23:59:59.999999","24:00:00",null]"#.into()
        )
    );
    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::time[])::text, \
             encode(array_send($1::text::time[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][1]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][2]);

    let directory = tempfile::tempdir().unwrap();
    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("time-array.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(),
        &mut sheet,
    )
    .unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("time[] result must remain text: {:?}", result.rows[0][0]);
    };
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_timetz_array_file_exports_preserve_offsets_for_calc_reimport() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY[\
        '00:00:00+15:59'::timetz, '23:59:59.999999-15:59'::timetz, \
        '12:34:56.123456+05:30'::timetz, '12:34:56.123456-05:30'::timetz, NULL]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("timetz[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, array_to_json({expression})::text, \
             encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("time with time zone[]".into()));
    assert_eq!(
        oracle.rows[0][1],
        Value::Text(
            r#"["00:00:00+15:59","23:59:59.999999-15:59","12:34:56.123456+05:30","12:34:56.123456-05:30",null]"#.into()
        )
    );
    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::timetz[])::text, \
             encode(array_send($1::text::timetz[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][1]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][2]);

    let directory = tempfile::tempdir().unwrap();
    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("timetz-array.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(),
        &mut sheet,
    )
    .unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("timetz[] result must remain text: {:?}", result.rows[0][0]);
    };
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_boolean_array_file_exports_preserve_values_for_calc_reimport() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY[true, false, NULL, true, false]::boolean[]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("bool[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, array_to_json({expression})::text, \
             encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("boolean[]".into()));
    assert_eq!(
        oracle.rows[0][1],
        Value::Text("[true,false,null,true,false]".into())
    );
    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::boolean[])::text, \
             encode(array_send($1::text::boolean[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][1]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][2]);

    let directory = tempfile::tempdir().unwrap();
    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("boolean-array.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(),
        &mut sheet,
    )
    .unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("boolean[] result must remain text: {:?}", result.rows[0][0]);
    };
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_int8_array_file_exports_preserve_precision_for_calc_reimport() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "'[0:3]={-9223372036854775808,9007199254740993,9223372036854775807,NULL}'::int8[]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("int8[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, array_to_json({expression})::text, \
             encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("bigint[]".into()));
    assert_eq!(
        oracle.rows[0][1],
        Value::Text("[-9223372036854775808,9007199254740993,9223372036854775807,null]".into())
    );
    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::int8[])::text, \
             encode(array_send($1::text::int8[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][1]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][2]);

    let directory = tempfile::tempdir().unwrap();
    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("int8-array.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(),
        &mut sheet,
    )
    .unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("int8[] result must remain text: {:?}", result.rows[0][0]);
    };
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_integer_arrays_file_exports_preserve_bounds_for_calc_reimport() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let smallint = "'[0:3]={-32768,0,32767,NULL}'::int2[]";
    let integer = "'[2:6]={-2147483648,-16777217,16777217,2147483647,NULL}'::int4[]";
    let result = connection
        .query(&format!("SELECT {smallint} AS smallints, {integer} AS integers"))
        .await
        .unwrap();
    assert_eq!(result.columns[0].data_type, "INT2[]");
    assert_eq!(result.columns[1].data_type, "INT4[]");
    let smallint_oracle = connection
        .query(&format!(
            "SELECT pg_typeof({smallint})::text, array_to_json({smallint})::text, \
             encode(array_send({smallint}), 'hex')"
        ))
        .await
        .unwrap();
    let integer_oracle = connection
        .query(&format!(
            "SELECT pg_typeof({integer})::text, array_to_json({integer})::text, \
             encode(array_send({integer}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(smallint_oracle.rows[0][0], Value::Text("smallint[]".into()));
    assert_eq!(integer_oracle.rows[0][0], Value::Text("integer[]".into()));
    assert_eq!(smallint_oracle.rows[0][1], Value::Text("[-32768,0,32767,null]".into()));
    assert_eq!(
        integer_oracle.rows[0][1],
        Value::Text("[-2147483648,-16777217,16777217,2147483647,null]".into())
    );
    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::int2[])::text, encode(array_send($1::text::int2[]), 'hex'), \
             array_to_json($2::text::int4[])::text, encode(array_send($2::text::int4[]), 'hex')",
            &[result.rows[0][0].clone(), result.rows[0][1].clone()],
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], smallint_oracle.rows[0][1]);
    assert_eq!(rebound.rows[0][1], smallint_oracle.rows[0][2]);
    assert_eq!(rebound.rows[0][2], integer_oracle.rows[0][1]);
    assert_eq!(rebound.rows[0][3], integer_oracle.rows[0][2]);

    let directory = tempfile::tempdir().unwrap();
    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("integer-arrays.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(),
        &mut sheet,
    )
    .unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    let (Value::Text(smallint_text), Value::Text(integer_text)) =
        (&result.rows[0][0], &result.rows[0][1])
    else {
        panic!("integer array results must remain text: {:?}", result.rows[0]);
    };
    for cell in ["A2", "B2"] {
        assert!(sheet.contains(&format!("<c r=\"{cell}\" t=\"s\">")), "{sheet}");
    }
    assert!(shared_strings.contains(smallint_text), "{shared_strings}");
    assert!(shared_strings.contains(integer_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_numeric_array_file_exports_preserve_exact_text() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY[1234567890123456789012345678901234567890::numeric, \
        0.123456789012345678901234567891::numeric, 1.2300::numeric, \
        'NaN'::numeric, 'Infinity'::numeric, '-Infinity'::numeric, NULL]::numeric[]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("numeric[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, {expression}::text, \
                    array_to_json({expression})::text, encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("numeric[]".into()));
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("numeric[] result must remain text: {:?}", result.rows[0][0]);
    };
    let Value::Text(native_text) = &oracle.rows[0][1] else {
        panic!("numeric[] native text oracle returned {:?}", oracle.rows[0][1]);
    };
    assert_eq!(
        driver_text,
        r#"{"1234567890123456789012345678901234567890","0.123456789012345678901234567891","1.2300","NaN","Infinity","-Infinity",NULL}"#
    );
    assert_eq!(
        native_text,
        "{1234567890123456789012345678901234567890,0.123456789012345678901234567891,1.2300,NaN,Infinity,-Infinity,NULL}"
    );
    let Value::Text(native_json) = &oracle.rows[0][2] else {
        panic!("numeric[] array_to_json oracle returned {:?}", oracle.rows[0][2]);
    };
    let json_elements = serde_json::from_str::<serde_json::Value>(native_json).unwrap();
    assert_eq!(json_elements.as_array().unwrap().len(), 7);
    assert!(json_elements[0].is_number());
    assert!(json_elements[1].is_number());
    assert!(json_elements[2].is_number());
    assert_eq!(json_elements[3], "NaN");
    assert_eq!(json_elements[4], "Infinity");
    assert_eq!(json_elements[5], "-Infinity");
    assert!(json_elements[6].is_null());

    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::numeric[])::text, \
                    encode(array_send($1::text::numeric[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][2]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][3]);

    let directory = tempfile::tempdir().unwrap();
    let csv_options = tablepro_core::export::CsvOptions::default();
    let json_path = directory.path().join("numeric-array.json");
    tablepro_core::export::write_result_file(
        &json_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Json,
            csv: &csv_options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let json_file: serde_json::Value = serde_json::from_slice(&std::fs::read(json_path).unwrap()).unwrap();
    assert_eq!(json_file[0]["value"], *driver_text);

    let csv_path = directory.path().join("numeric-array.csv");
    tablepro_core::export::write_result_file(
        &csv_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Csv,
            csv: &csv_options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut csv = csv::Reader::from_path(csv_path).unwrap();
    assert_eq!(csv.headers().unwrap().iter().collect::<Vec<_>>(), ["value"]);
    assert_eq!(&csv.records().next().unwrap().unwrap()[0], driver_text);

    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("numeric-array.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &csv_options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");

    connection
        .execute("CREATE TABLE numeric_array_filewriter_target (value numeric[])")
        .await
        .unwrap();
    let sql_path = directory.path().join("numeric-array.sql");
    tablepro_core::export::write_result_file(
        &sql_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Sql,
            csv: &csv_options,
            sql: Some(tablepro_core::export::SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "numeric_array_filewriter_target",
            }),
        },
        || false,
        |_| {},
    )
    .unwrap();
    connection
        .execute(&std::fs::read_to_string(sql_path).unwrap())
        .await
        .unwrap();
    let restored = connection
        .query(
            "SELECT value::text, array_to_json(value)::text, \
                    encode(array_send(value), 'hex') FROM numeric_array_filewriter_target",
        )
        .await
        .unwrap();
    assert_eq!(restored.rows, vec![oracle.rows[0][1..].to_vec()]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_float8_array_file_exports_preserve_bits() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY[1.0000000000000002::float8, '-0'::float8, \
        '4.9406564584124654e-324'::float8, 'NaN'::float8, 'Infinity'::float8, \
        '-Infinity'::float8, NULL]::float8[]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("float8[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, array_to_json({expression})::text, \
                    encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("double precision[]".into()));
    let Value::Text(native_json) = &oracle.rows[0][1] else {
        panic!("float8[] array_to_json returned {:?}", oracle.rows[0][1]);
    };
    let elements = serde_json::from_str::<serde_json::Value>(native_json).unwrap();
    let elements = elements.as_array().unwrap();
    assert_eq!(elements.len(), 7);
    assert_eq!(elements[0].as_f64().unwrap().to_bits(), 1.0000000000000002f64.to_bits());
    assert_eq!(elements[2].as_f64().unwrap().to_bits(), 1);
    assert_eq!(elements[3], "NaN");
    assert_eq!(elements[4], "Infinity");
    assert_eq!(elements[5], "-Infinity");
    assert!(elements[6].is_null());

    let element_wire = connection
        .query(&format!(
            "SELECT ordinality, item IS NULL, encode(float8send(item), 'hex') \
             FROM unnest({expression}) WITH ORDINALITY AS element(item, ordinality) ORDER BY ordinality"
        ))
        .await
        .unwrap();
    assert_eq!(element_wire.rows[0][2], Value::Text("3ff0000000000001".into()));
    // array_to_json renders negative zero as 0; float8send is the lossless oracle.
    assert_eq!(element_wire.rows[1][2], Value::Text("8000000000000000".into()));
    assert_eq!(element_wire.rows[2][2], Value::Text("0000000000000001".into()));
    assert_eq!(element_wire.rows[6][1], Value::Bool(true));
    assert_eq!(element_wire.rows[6][2], Value::Null);

    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::float8[])::text, encode(array_send($1::text::float8[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][1]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][2]);

    let directory = tempfile::tempdir().unwrap();
    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("float8-array.xlsx"));
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("float8[] result must remain text: {:?}", result.rows[0][0]);
    };
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_timestamptz_array_file_exports_preserve_instants() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY['2024-11-03 01:30:00.123456-04'::timestamptz, \
        '2024-11-03 01:30:00.123456-05'::timestamptz, \
        '0001-01-01 12:34:56+00 BC'::timestamptz, \
        'infinity'::timestamptz, '-infinity'::timestamptz, NULL]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("timestamptz[]", &result);

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, {expression}::text, \
                    array_to_json({expression})::text, encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("timestamp with time zone[]".into()));
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("timestamptz[] result must remain text: {:?}", result.rows[0][0]);
    };
    let Value::Text(native_json) = &oracle.rows[0][2] else {
        panic!("timestamptz[] array_to_json oracle returned {:?}", oracle.rows[0][2]);
    };
    let json_elements = serde_json::from_str::<serde_json::Value>(native_json).unwrap();
    assert_eq!(json_elements.as_array().unwrap().len(), 6);
    assert!(json_elements[0].as_str().unwrap().contains("2024-11-03"));
    assert!(json_elements[1].as_str().unwrap().contains("2024-11-03"));
    assert_ne!(json_elements[0], json_elements[1]);
    assert!(json_elements[2].as_str().unwrap().contains("0001-01-01"));
    assert_eq!(json_elements[3], "infinity");
    assert_eq!(json_elements[4], "-infinity");
    assert!(json_elements[5].is_null());

    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::timestamptz[])::text, \
                    encode(array_send($1::text::timestamptz[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][2]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][3]);

    let directory = tempfile::tempdir().unwrap();
    let csv_options = tablepro_core::export::CsvOptions::default();
    let json_path = directory.path().join("timestamptz-array.json");
    tablepro_core::export::write_result_file(
        &json_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Json,
            csv: &csv_options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let json_file: serde_json::Value = serde_json::from_slice(&std::fs::read(json_path).unwrap()).unwrap();
    assert_eq!(json_file[0]["value"], *driver_text);

    let csv_path = directory.path().join("timestamptz-array.csv");
    tablepro_core::export::write_result_file(
        &csv_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Csv,
            csv: &csv_options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut csv = csv::Reader::from_path(csv_path).unwrap();
    assert_eq!(csv.headers().unwrap().iter().collect::<Vec<_>>(), ["value"]);
    assert_eq!(&csv.records().next().unwrap().unwrap()[0], driver_text);

    let xlsx_path = directory.path().join("timestamptz-array.xlsx");
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &csv_options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    if let Some(path) = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT") {
        std::fs::copy(&xlsx_path, path).unwrap();
    }
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");

    connection
        .execute("CREATE TABLE timestamptz_array_filewriter_target (value timestamptz[])")
        .await
        .unwrap();
    let sql_path = directory.path().join("timestamptz-array.sql");
    tablepro_core::export::write_result_file(
        &sql_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Sql,
            csv: &csv_options,
            sql: Some(tablepro_core::export::SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "timestamptz_array_filewriter_target",
            }),
        },
        || false,
        |_| {},
    )
    .unwrap();
    connection
        .execute(&std::fs::read_to_string(sql_path).unwrap())
        .await
        .unwrap();
    let restored = connection
        .query(
            "SELECT value::text, array_to_json(value)::text, \
                    encode(array_send(value), 'hex') FROM timestamptz_array_filewriter_target",
        )
        .await
        .unwrap();
    assert_eq!(restored.rows, vec![oracle.rows[0][1..].to_vec()]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_interval_array_file_exports_preserve_values_under_intervalstyle() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE TABLE interval_array_filewriter_target (value interval[])")
        .await
        .unwrap();

    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute("SET LOCAL intervalstyle = 'postgres_verbose'")
        .await
        .unwrap();
    let expression = "ARRAY['1 year 2 mons 3 days 04:05:06.123456'::interval, \
        '-1 year +2 mons -3 days -04:05:06.654321'::interval, \
        '0.000001 seconds'::interval, '00:00:00'::interval, NULL]";
    let result = transaction
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("interval[]", &result);

    let oracle = transaction
        .query(&format!(
            "SELECT pg_typeof({expression})::text, {expression}::text, \
                    array_to_json({expression})::text, encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("interval[]".into()));
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("interval[] result must remain text: {:?}", result.rows[0][0]);
    };
    let Value::Text(native_json) = &oracle.rows[0][2] else {
        panic!("interval[] array_to_json oracle returned {:?}", oracle.rows[0][2]);
    };
    let json_elements = serde_json::from_str::<serde_json::Value>(native_json).unwrap();
    assert_eq!(json_elements.as_array().unwrap().len(), 5);
    assert_eq!(json_elements[4], serde_json::Value::Null);
    assert!(json_elements[0].as_str().unwrap().contains("1 year"));
    assert!(json_elements[1].as_str().unwrap().contains("ago"));
    assert_ne!(json_elements[0], json_elements[1]);

    let rebound = transaction
        .query_params(
            "SELECT array_to_json($1::text::interval[])::text, \
                    encode(array_send($1::text::interval[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][2]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][3]);

    let directory = tempfile::tempdir().unwrap();
    let csv_options = tablepro_core::export::CsvOptions::default();
    let json_path = directory.path().join("interval-array.json");
    tablepro_core::export::write_result_file(
        &json_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Json,
            csv: &csv_options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let json_file: serde_json::Value = serde_json::from_slice(&std::fs::read(json_path).unwrap()).unwrap();
    assert_eq!(json_file[0]["value"], *driver_text);

    let csv_path = directory.path().join("interval-array.csv");
    tablepro_core::export::write_result_file(
        &csv_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Csv,
            csv: &csv_options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut csv = csv::Reader::from_path(csv_path).unwrap();
    assert_eq!(csv.headers().unwrap().iter().collect::<Vec<_>>(), ["value"]);
    assert_eq!(&csv.records().next().unwrap().unwrap()[0], driver_text);

    let xlsx_path = directory.path().join("interval-array.xlsx");
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &csv_options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    if let Some(path) = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT") {
        std::fs::copy(&xlsx_path, path).unwrap();
    }
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(driver_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");

    let sql_path = directory.path().join("interval-array.sql");
    tablepro_core::export::write_result_file(
        &sql_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Sql,
            csv: &csv_options,
            sql: Some(tablepro_core::export::SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "interval_array_filewriter_target",
            }),
        },
        || false,
        |_| {},
    )
    .unwrap();
    transaction
        .execute(&std::fs::read_to_string(sql_path).unwrap())
        .await
        .unwrap();
    let restored = transaction
        .query(
            "SELECT value::text, array_to_json(value)::text, \
                    encode(array_send(value), 'hex') FROM interval_array_filewriter_target",
        )
        .await
        .unwrap();
    assert_eq!(restored.rows, vec![oracle.rows[0][1..].to_vec()]);
    transaction.rollback().await.unwrap();
    assert_eq!(
        connection
            .query("SELECT count(*)::bigint FROM interval_array_filewriter_target")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(0)]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_unlisted_builtin_arrays_refuse_with_native_oracles() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let cases = [
        (
            "inet[]",
            "ARRAY['192.0.2.1/24'::inet, NULL]",
            "{192.0.2.1/24,NULL}",
            "[\"192.0.2.1/24\",null]",
        ),
        (
            "cidr[]",
            "ARRAY['192.0.2.0/24'::cidr, NULL]",
            "{192.0.2.0/24,NULL}",
            "[\"192.0.2.0/24\",null]",
        ),
        (
            "macaddr[]",
            "ARRAY['08:00:2b:01:02:03'::macaddr, NULL]",
            "{08:00:2b:01:02:03,NULL}",
            "[\"08:00:2b:01:02:03\",null]",
        ),
        (
            "macaddr8[]",
            "ARRAY['08:00:2b:01:02:03:04:05'::macaddr8, NULL]",
            "{08:00:2b:01:02:03:04:05,NULL}",
            "[\"08:00:2b:01:02:03:04:05\",null]",
        ),
        (
            "pg_lsn[]",
            "ARRAY['0/16B6C50'::pg_lsn, NULL]",
            "{0/16B6C50,NULL}",
            "[\"0/16B6C50\",null]",
        ),
        ("bit[]", "ARRAY[B'101'::bit(3), NULL]", "{101,NULL}", "[\"101\",null]"),
        (
            "bit varying[]",
            "ARRAY[B'101'::varbit, NULL]",
            "{101,NULL}",
            "[\"101\",null]",
        ),
    ];

    for (expected_type, expression, expected_text, expected_json) in cases {
        let sql = format!(
            "SELECT ({expression}) AS value, pg_typeof(({expression}))::text, \
             ({expression})::text, array_to_json(({expression}))::text"
        );
        let result = connection.query(&sql).await.expect("native array oracle query");
        let value = &result.rows[0][0];
        assert!(
            matches!(value, Value::Undecodable(type_name) if !type_name.is_empty()),
            "{expected_type}: {value:?}"
        );
        assert_eq!(result.rows[0][1], Value::Text(expected_type.into()), "{expected_type}");
        assert_eq!(result.rows[0][2], Value::Text(expected_text.into()), "{expected_type}");
        assert_eq!(result.rows[0][3], Value::Text(expected_json.into()), "{expected_type}");
        assert!(
            tablepro_core::sql_literal::render_sql_literal("postgres", value).is_err(),
            "{expected_type}"
        );
        assert!(
            connection
                .query_params("SELECT $1", std::slice::from_ref(value))
                .await
                .is_err(),
            "{expected_type}"
        );
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_builtin_array_oid_census_matches_decode_allowlist() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let catalog = connection
        .query(
            "SELECT oid::bigint, format_type(typarray, NULL) \
             FROM pg_catalog.pg_type \
             WHERE typnamespace = 'pg_catalog'::regnamespace \
               AND typelem = 0 AND typarray <> 0 AND typisdefined \
               AND typtype IN ('b', 'c', 'd', 'e', 'm', 'r') \
             ORDER BY oid",
        )
        .await
        .expect("enumerate built-in array element OIDs");
    let supported_oids = [
        16, 17, 19, 20, 21, 23, 25, 26, 700, 701, 1042, 1043, 1082, 1083, 1114, 1184, 1186, 1266, 1700, 2950,
    ];
    let mut seen_supported = Vec::new();
    let mut refused = 0;
    let mut blocked = Vec::new();

    for row in catalog.rows {
        let Value::Int(oid) = row[0] else {
            panic!("unexpected catalog OID: {:?}", row[0])
        };
        let Value::Text(array_type) = &row[1] else {
            panic!("unexpected array type: {:?}", row[1])
        };
        let expression = format!("ARRAY[NULL]::{array_type}");
        let oracle = connection
            .query(&format!(
                "SELECT pg_typeof({expression})::text, {expression}::text, \
                 array_to_json({expression})::text"
            ))
            .await
            .unwrap_or_else(|error| panic!("native {array_type} oracle (element OID {oid}): {error:?}"));
        assert_eq!(oracle.rows[0][0], Value::Text(array_type.clone()), "OID {oid}");
        assert_eq!(oracle.rows[0][1], Value::Text("{NULL}".into()), "OID {oid}");
        assert_eq!(oracle.rows[0][2], Value::Text("[null]".into()), "OID {oid}");

        let direct = connection.query(&format!("SELECT {expression} AS value")).await;
        let result = match direct {
            Ok(result) => result,
            Err(error) => {
                let details = format!("{error:?}");
                let known_metadata_failure = details.contains("typcategory") && details.contains("category code 90");
                let known_no_binary_output = details.contains("no binary output function available");
                let known_multirange_metadata_failure = details.contains("unknown type code 109");
                assert!(
                    known_metadata_failure || known_no_binary_output || known_multirange_metadata_failure,
                    "{array_type} (element OID {oid}) unexpected error: {details}"
                );
                blocked.push((array_type.clone(), details));
                continue;
            }
        };
        let value = &result.rows[0][0];
        if supported_oids.contains(&(oid as u32)) {
            seen_supported.push(oid as u32);
            assert_eq!(value, &Value::Text("{NULL}".into()), "supported {array_type}");
            assert!(tablepro_core::sql_literal::render_sql_literal("postgres", value).is_ok());
        } else {
            refused += 1;
            assert!(matches!(value, Value::Undecodable(_)), "{array_type}: {value:?}");
            assert!(tablepro_core::sql_literal::render_sql_literal("postgres", value).is_err());
            assert!(
                connection
                    .query_params("SELECT $1", std::slice::from_ref(value))
                    .await
                    .is_err()
            );
        }
    }

    seen_supported.sort_unstable();
    let catalog_supported_oids: Vec<_> = supported_oids.iter().copied().filter(|oid| *oid != 19).collect();
    assert_eq!(
        seen_supported, catalog_supported_oids,
        "the catalog must cover all allowlisted built-in arrays except name[]"
    );
    let name_array = connection
        .query("SELECT ARRAY[NULL]::name[]")
        .await
        .expect("name[] decoder allowlist contract");
    assert_eq!(name_array.rows[0][0], Value::Text("{NULL}".into()));
    assert!(
        refused >= 7,
        "the catalog census should cover the explicit unlisted-array contract set"
    );
    let blocked_names: Vec<_> = blocked.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        blocked_names,
        [
            "pg_type[]",
            "pg_proc[]",
            "pg_class[]",
            "aclitem[]",
            "gtsvector[]",
            "int4multirange[]",
            "nummultirange[]",
            "tsmultirange[]",
            "tstzmultirange[]",
            "datemultirange[]",
            "int8multirange[]",
            "pg_attrdef[]",
            "pg_constraint[]",
            "pg_index[]",
            "pg_statistic_ext[]",
            "pg_statistic_ext_data[]",
            "pg_rewrite[]",
            "pg_trigger[]",
            "pg_policy[]",
            "pg_partitioned_table[]",
            "pg_publication_rel[]",
            "pg_stats_ext[]",
        ],
        "every driver-level blocker must be explicit and stable: {blocked:?}"
    );
    assert!(
        blocked
            .iter()
            .filter(|(_, error)| error.contains("category code 90"))
            .all(|(name, _)| { name.ends_with("[]") && !name.contains("multirange") })
    );
    assert!(
        blocked
            .iter()
            .filter(|(_, error)| error.contains("no binary output function available"))
            .all(|(name, _)| { matches!(name.as_str(), "aclitem[]" | "gtsvector[]") })
    );
    assert!(
        blocked
            .iter()
            .filter(|(_, error)| error.contains("unknown type code 109"))
            .all(|(name, _)| { name.contains("multirange") })
    );
    for (name, error) in &blocked {
        if matches!(name.as_str(), "aclitem[]" | "gtsvector[]") {
            assert!(error.contains("no binary output function available"), "{name}: {error}");
        } else if name.contains("multirange") {
            assert!(error.contains("unknown type code 109"), "{name}: {error}");
        } else {
            assert!(
                error.contains("typcategory") && error.contains("category code 90"),
                "{name}: {error}"
            );
        }
    }
}
