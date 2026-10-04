#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{Connection, OperationControl, QueryResult, Value};
use tokio_util::sync::CancellationToken;

pub async fn assert_array_contract(connection: &dyn Connection) {
    let control = OperationControl::new(CancellationToken::new(), None);
    let mut session = connection.open_session().await.unwrap();
    session
        .query_params_controlled("SET TIME ZONE 'Asia/Kathmandu'", &[], &control)
        .await
        .unwrap();
    for (kind, expression) in [
        ("int4[]", "NULL::int4[]"),
        ("int4[]", "ARRAY[]::int4[]"),
        ("int4[]", "'{{{{{{1,NULL}}}}}}'::int4[]"),
        ("name[]", "ARRAY['alpha','',NULL]::name[]"),
        ("oid[]", "ARRAY[0,4294967295,NULL]::oid[]"),
        ("int4[]", "ARRAY[1,NULL,-2147483648,2147483647]"),
        ("int2[]", "ARRAY[-32768,0,32767]::int2[]"),
        ("int8[]", "ARRAY[-9223372036854775808,9223372036854775807]::int8[]"),
        ("int4[]", "'[0:1][-2:0]={{1,NULL,3},{4,5,6}}'::int4[]"),
        (
            "text[]",
            "ARRAY[NULL,'NULL','null','','{}','a,b',' leading ', '漢字 😀', $$a\"b$$, $$a\\b$$, E'line\nnext', $$x'); DROP TABLE t; --$$]",
        ),
        ("text[]", "ARRAY[['a',NULL],['','NULL']]"),
        ("varchar[]", "ARRAY['x','',NULL]::varchar[]"),
        ("bpchar[]", "ARRAY['x',' y']::char(3)[]"),
        (
            "numeric[]",
            "ARRAY[1234567890123456789012345678901234567890,0.123456789012345678901234567891,1.2300,NULL,'NaN'::numeric,'Infinity'::numeric]",
        ),
        ("bool[]", "ARRAY[true,false,NULL]"),
        (
            "float4[]",
            "ARRAY[0.1,'-0','NaN','Infinity','-Infinity',NULL]::float4[]",
        ),
        (
            "float8[]",
            "ARRAY[1e-200,1.0000000000000002,'-0','NaN','Infinity',NULL]::float8[]",
        ),
        ("uuid[]", "ARRAY['12345678-1234-5678-90ab-1234567890ab',NULL]::uuid[]"),
        ("bytea[]", "ARRAY[decode('00ff275c','hex'),decode('','hex'),NULL]"),
        (
            "date[]",
            "ARRAY['0002-12-31 BC','10000-01-01','infinity','-infinity',NULL]::date[]",
        ),
        ("time[]", "ARRAY['00:00:00','24:00:00','23:59:59.999999',NULL]::time[]"),
        (
            "timetz[]",
            "ARRAY['00:00:00+15:59:59','24:00:00-15:59:59',NULL]::timetz[]",
        ),
        (
            "timestamp[]",
            "ARRAY['0001-01-01 00:00:00.000001 BC','10000-01-01 23:59:59.999999','infinity','-infinity',NULL]::timestamp[]",
        ),
        (
            "timestamptz[]",
            "ARRAY['2024-11-03 01:30:00-04','2024-11-03 01:30:00-05','0001-01-01 12:34:56+00 BC','infinity','-infinity',NULL]::timestamptz[]",
        ),
        (
            "date[]",
            "'[0:1][-1:0]={{2000-01-01,NULL},{infinity,-infinity}}'::date[]",
        ),
        (
            "interval[]",
            "ARRAY['-1 month +2 days +0.000001 seconds','+1 month -2 days -0.000001 seconds','-9223372036854.775808 seconds',NULL]::interval[]",
        ),
    ] {
        let sql = format!("SELECT ({expression}) AS value, encode(array_send({expression}), 'hex') AS wire");
        let result = connection.query(&sql).await.unwrap();
        assert_array_value(kind, &result);
        crate::wire_round_trip::assert_wire_round_trip(
            session.as_mut(),
            &control,
            kind,
            &result.columns[0],
            &result.rows[0][0],
            &result.rows[0][1],
        )
        .await;
        let session_result = session.query_params_controlled(&sql, &[], &control).await.unwrap();
        assert_eq!(session_result.rows, result.rows, "{kind}: session");
        assert_eq!(session_result.columns[0].data_type, result.columns[0].data_type);
        let bound = session
            .query_params_controlled(
                &format!("SELECT encode(array_send($1::text::{kind}), 'hex')"),
                &result.rows[0][..1],
                &control,
            )
            .await
            .unwrap();
        assert_eq!(bound.rows[0][0], result.rows[0][1], "{kind}: non-UTC session import");
    }
}

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

    let xlsx_path = directory.path().join("array.xlsx");
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

    let xlsx_path = directory.path().join("numeric-array.xlsx");
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

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_json_array_elements_are_explicitly_unsupported() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY['{\"a\": 1}'::jsonb, 'null'::jsonb]";
    let result = connection
        .query(&format!(
            "SELECT {expression} AS value, pg_typeof({expression})::text AS array_type, \
             array_to_json({expression})::text AS json_text, \
             array_to_json({expression})::jsonb = '[{{\"a\": 1}}, null]'::jsonb AS server_match"
        ))
        .await
        .unwrap();

    let value = &result.rows[0][0];
    assert!(matches!(value, Value::Undecodable(_)), "{value:?}");
    assert_eq!(result.rows[0][1], Value::Text("jsonb[]".into()));
    assert_eq!(result.rows[0][2], Value::Text("[{\"a\": 1},null]".into()));
    assert_eq!(result.rows[0][3], Value::Bool(true));
    assert!(tablepro_core::sql_literal::render_sql_literal("postgres", value).is_err());
    assert!(
        connection
            .query_params("SELECT $1", std::slice::from_ref(value))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_json_text_array_is_explicitly_unsupported() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY['{\"a\": 1}'::json, 'null'::json]";
    let result = connection
        .query(&format!(
            "SELECT {expression} AS value, pg_typeof({expression})::text AS array_type, \
             array_to_json({expression})::text AS json_text, \
             array_to_json({expression})::jsonb = '[{{\"a\": 1}}, null]'::jsonb AS server_match"
        ))
        .await
        .unwrap();

    let value = &result.rows[0][0];
    assert!(matches!(value, Value::Undecodable(_)), "{value:?}");
    assert_eq!(result.rows[0][1], Value::Text("json[]".into()));
    assert_eq!(result.rows[0][2], Value::Text("[{\"a\": 1},null]".into()));
    assert_eq!(result.rows[0][3], Value::Bool(true));
    assert!(tablepro_core::sql_literal::render_sql_literal("postgres", value).is_err());
    assert!(
        connection
            .query_params("SELECT $1", std::slice::from_ref(value))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_projection_preserves_literal_null_and_sql_null() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE TYPE value_contract_array_enum AS ENUM ('NULL', '東京', 'o''brien')")
        .await
        .unwrap();
    let expression = "ARRAY['NULL'::value_contract_array_enum, \
        '東京'::value_contract_array_enum, \
        'o''brien'::value_contract_array_enum, NULL]";
    let result = connection
        .query(&format!(
            "SELECT pg_typeof(value)::text AS array_type, value::text AS exact_text, \
             array_to_json(value)::jsonb = \
               '[\"NULL\",\"東京\",\"o''brien\",null]'::jsonb AS server_match \
             FROM (SELECT {expression} AS value) source"
        ))
        .await
        .unwrap();

    assert_eq!(result.rows[0][0], Value::Text("value_contract_array_enum[]".into()));
    assert_eq!(result.rows[0][1], Value::Text(r#"{"NULL",東京,o'brien,NULL}"#.into()));
    assert_eq!(result.rows[0][2], Value::Bool(true));
    let direct_projection = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_eq!(
        direct_projection.rows,
        vec![vec![Value::Text(r#"{"NULL","東京","o'brien",NULL}"#.into())]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_file_exports_preserve_labels() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute(
            "CREATE TYPE value_contract_file_enum AS ENUM \
             ('NULL', '', '東京', 'a,b', 'a\"b', '<tag>&')",
        )
        .await
        .unwrap();
    let expression = "ARRAY['NULL'::value_contract_file_enum, \
        ''::value_contract_file_enum, '東京'::value_contract_file_enum, \
        'a,b'::value_contract_file_enum, 'a\"b'::value_contract_file_enum, \
        '<tag>&'::value_contract_file_enum, NULL]";
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_eq!(result.columns[0].data_type, "value_contract_file_enum[]");
    assert!(matches!(result.rows[0][0], Value::Text(_)));

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, {expression}::text, \
                    array_to_json({expression})::text, encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("value_contract_file_enum[]".into()));
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("enum[] result must remain text: {:?}", result.rows[0][0]);
    };
    let Value::Text(native_json) = &oracle.rows[0][2] else {
        panic!("enum[] array_to_json oracle returned {:?}", oracle.rows[0][2]);
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(native_json).unwrap(),
        serde_json::json!(["NULL", "", "東京", "a,b", "a\"b", "<tag>&", null])
    );
    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::value_contract_file_enum[])::text, \
                    encode(array_send($1::text::value_contract_file_enum[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][2]);
    assert_eq!(rebound.rows[0][1], oracle.rows[0][3]);

    let directory = tempfile::tempdir().unwrap();
    let csv_options = tablepro_core::export::CsvOptions::default();
    let json_path = directory.path().join("enum-array.json");
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

    let csv_path = directory.path().join("enum-array.csv");
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

    let xlsx_path = directory.path().join("enum-array.xlsx");
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
    let xlsx_text = driver_text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    assert!(sheet.contains("<c r=\"A2\" t=\"s\">"), "{sheet}");
    assert!(shared_strings.contains(&xlsx_text), "{shared_strings}");
    assert!(!sheet.contains("<f>"), "{sheet}");

    connection
        .execute("CREATE TABLE enum_array_filewriter_target (value value_contract_file_enum[])")
        .await
        .unwrap();
    let sql_path = directory.path().join("enum-array.sql");
    tablepro_core::export::write_result_file(
        &sql_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Sql,
            csv: &csv_options,
            sql: Some(tablepro_core::export::SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "enum_array_filewriter_target",
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
                    encode(array_send(value), 'hex') FROM enum_array_filewriter_target",
        )
        .await
        .unwrap();
    assert_eq!(restored.rows, vec![oracle.rows[0][1..].to_vec()]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_csv_import_preserves_labels_and_siblings() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE SCHEMA value_contract_enum_array_import")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_enum_array_import.label AS ENUM \
             ('NULL', '', '東京', 'a,b', 'a\"b', '<tag>&', 'sibling')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_enum_array_import.target \
             (id integer PRIMARY KEY, labels value_contract_enum_array_import.label[])",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_enum_array_import.target VALUES \
             (1, ARRAY['sibling'::value_contract_enum_array_import.label])",
        )
        .await
        .unwrap();
    let sibling_wire = connection
        .query(
            "SELECT encode(array_send(labels), 'hex') \
             FROM value_contract_enum_array_import.target WHERE id = 1",
        )
        .await
        .unwrap()
        .rows[0][0]
        .clone();

    let source = connection
        .query(
            "SELECT 3 AS id, ARRAY[\
                'NULL'::value_contract_enum_array_import.label, \
                ''::value_contract_enum_array_import.label, \
                '東京'::value_contract_enum_array_import.label, \
                'a,b'::value_contract_enum_array_import.label, \
                'a\"b'::value_contract_enum_array_import.label, \
                '<tag>&'::value_contract_enum_array_import.label, \
                NULL::value_contract_enum_array_import.label\
            ] AS labels",
        )
        .await
        .unwrap();
    let native = connection
        .query(
            "SELECT pg_typeof(labels)::text, labels::text, array_to_json(labels)::text, \
                    encode(array_send(labels), 'hex') \
             FROM (SELECT ARRAY[\
                'NULL'::value_contract_enum_array_import.label, \
                ''::value_contract_enum_array_import.label, \
                '東京'::value_contract_enum_array_import.label, \
                'a,b'::value_contract_enum_array_import.label, \
                'a\"b'::value_contract_enum_array_import.label, \
                '<tag>&'::value_contract_enum_array_import.label, \
                NULL::value_contract_enum_array_import.label\
             ] AS labels) AS source",
        )
        .await
        .unwrap();
    assert_eq!(
        native.rows[0][0],
        Value::Text("value_contract_enum_array_import.label[]".into())
    );
    let csv_options = tablepro_core::export::CsvOptions::default();
    let csv = tablepro_core::export::render_csv(&source.columns, &source.rows, &csv_options);
    let import_options = tablepro_core::import::CsvImportOptions::default();
    let sheet = tablepro_core::import::read_csv(csv.as_bytes(), &import_options, None).unwrap();
    let columns = connection
        .fetch_columns(Some("value_contract_enum_array_import"), "target")
        .await
        .unwrap();
    assert_eq!(columns[1].data_type, "value_contract_enum_array_import.label[]");
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "value_contract_enum_array_import".into(),
            name: "label".into(),
        })
    );
    let mapping = [Some(0), Some(1)];
    let plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "postgres",
            schema: Some("value_contract_enum_array_import"),
            table: "target",
            columns: &columns,
            mapping: &mapping,
        },
        &sheet,
        &import_options,
    )
    .unwrap();
    assert_eq!(
        plan.statement,
        "INSERT INTO \"value_contract_enum_array_import\".\"target\" \
         (\"id\", \"labels\") VALUES ($1, $2::text::\"value_contract_enum_array_import\".\"label\"[])"
    );
    connection.execute_params(&plan.statement, &plan.rows[0]).await.unwrap();

    let restored = connection
        .query(
            "SELECT id, pg_typeof(labels)::text, labels::text, array_to_json(labels)::text, \
                    encode(array_send(labels), 'hex') \
             FROM value_contract_enum_array_import.target ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(restored.rows.len(), 2);
    assert_eq!(
        restored.rows[0],
        vec![
            Value::Int(1),
            Value::Text("value_contract_enum_array_import.label[]".into()),
            Value::Text("{sibling}".into()),
            Value::Text("[\"sibling\"]".into()),
            sibling_wire,
        ]
    );
    assert_eq!(
        restored.rows[1],
        vec![
            Value::Int(3),
            native.rows[0][0].clone(),
            native.rows[0][1].clone(),
            native.rows[0][2].clone(),
            native.rows[0][3].clone(),
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_csv_import_preserves_nulls_and_shape() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE SCHEMA value_contract_enum_array_shape")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_enum_array_shape.label AS ENUM ('NULL', '', '東京', 'sibling')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_enum_array_shape.target \
             (id integer PRIMARY KEY, labels value_contract_enum_array_shape.label[])",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_enum_array_shape.target VALUES \
             (99, ARRAY['sibling'::value_contract_enum_array_shape.label])",
        )
        .await
        .unwrap();
    let sibling_wire = connection
        .query(
            "SELECT encode(array_send(labels), 'hex') \
             FROM value_contract_enum_array_shape.target WHERE id = 99",
        )
        .await
        .unwrap()
        .rows[0][0]
        .clone();

    let source = connection
        .query(
            "SELECT 1 AS id, NULL::value_contract_enum_array_shape.label[] AS labels \
             UNION ALL SELECT 2, ARRAY[]::value_contract_enum_array_shape.label[] \
             UNION ALL SELECT 3, ARRAY[NULL::value_contract_enum_array_shape.label] \
             UNION ALL SELECT 4, '[0:3]={\"NULL\",NULL,\"\",東京}'::value_contract_enum_array_shape.label[] \
             UNION ALL SELECT 5, ARRAY[\
                 ['NULL'::value_contract_enum_array_shape.label, NULL::value_contract_enum_array_shape.label], \
                 [''::value_contract_enum_array_shape.label, '東京'::value_contract_enum_array_shape.label]\
             ]::value_contract_enum_array_shape.label[] \
             ORDER BY id",
        )
        .await
        .unwrap();
    let native = connection
        .query(
            "SELECT id, pg_typeof(labels)::text, array_dims(labels), \
                    array_to_json(labels)::text, encode(array_send(labels), 'hex') \
             FROM (\
                 SELECT 1 AS id, NULL::value_contract_enum_array_shape.label[] AS labels \
                 UNION ALL SELECT 2, ARRAY[]::value_contract_enum_array_shape.label[] \
                 UNION ALL SELECT 3, ARRAY[NULL::value_contract_enum_array_shape.label] \
                 UNION ALL SELECT 4, '[0:3]={\"NULL\",NULL,\"\",東京}'::value_contract_enum_array_shape.label[] \
                 UNION ALL SELECT 5, ARRAY[\
                     ['NULL'::value_contract_enum_array_shape.label, NULL::value_contract_enum_array_shape.label], \
                     [''::value_contract_enum_array_shape.label, '東京'::value_contract_enum_array_shape.label]\
                 ]::value_contract_enum_array_shape.label[]\
             ) AS source ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(source.rows.len(), 5);
    assert_eq!(native.rows.len(), 5);
    assert_eq!(native.rows[0][2], Value::Null);
    assert_eq!(native.rows[1][2], Value::Null);
    assert_eq!(native.rows[0][3], Value::Null);
    assert_eq!(native.rows[1][3], Value::Text("[]".into()));
    assert_ne!(native.rows[0][4], native.rows[1][4]);
    assert_eq!(native.rows[2][3], Value::Text("[null]".into()));
    assert_eq!(native.rows[3][2], Value::Text("[0:3]".into()));
    assert_eq!(native.rows[4][2], Value::Text("[1:2][1:2]".into()));

    let csv_options = tablepro_core::export::CsvOptions {
        null_marker: Some("\\N".into()),
        ..Default::default()
    };
    let csv = tablepro_core::export::render_csv(&source.columns, &source.rows, &csv_options);
    let import_options = tablepro_core::import::CsvImportOptions {
        null_marker: "\\N".into(),
        ..Default::default()
    };
    let sheet = tablepro_core::import::read_csv(csv.as_bytes(), &import_options, None).unwrap();
    let columns = connection
        .fetch_columns(Some("value_contract_enum_array_shape"), "target")
        .await
        .unwrap();
    let mapping = [Some(0), Some(1)];
    let plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "postgres",
            schema: Some("value_contract_enum_array_shape"),
            table: "target",
            columns: &columns,
            mapping: &mapping,
        },
        &sheet,
        &import_options,
    )
    .unwrap();
    assert_eq!(plan.rows.len(), 5);
    for row in &plan.rows {
        connection.execute_params(&plan.statement, row).await.unwrap();
    }

    let restored = connection
        .query(
            "SELECT id, pg_typeof(labels)::text, array_dims(labels), \
                    array_to_json(labels)::text, encode(array_send(labels), 'hex') \
             FROM value_contract_enum_array_shape.target ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(restored.rows.len(), 6);
    assert_eq!(&restored.rows[..5], native.rows.as_slice());
    assert_eq!(restored.rows[5][0], Value::Int(99));
    assert_eq!(restored.rows[5][3], Value::Text("[\"sibling\"]".into()));
    assert_eq!(restored.rows[5][4], sibling_wire);
}

pub async fn assert_array_grid_edit(connection: &dyn Connection) {
    connection
        .execute("CREATE TABLE array_grid_edit (id integer PRIMARY KEY, value integer[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO array_grid_edit VALUES (1, ARRAY[1,2])")
        .await
        .unwrap();
    let mut row = connection.query("SELECT * FROM array_grid_edit").await.unwrap();
    row.columns[0].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "array_grid_edit",
        &row.columns,
        &[(1, Value::Text("{3,NULL,5}".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);
    let updated = connection
        .query("SELECT value = ARRAY[3,NULL,5]::integer[] FROM array_grid_edit WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(updated.rows, vec![vec![Value::Bool(true)]]);

    const TEXT_ARRAY: &str = r#"{"plain",NULL,"quote \" slash \\, comma"}"#;
    connection
        .execute("CREATE TABLE text_array_grid_edit (id integer PRIMARY KEY, value text[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO text_array_grid_edit VALUES (1, ARRAY['before']::text[])")
        .await
        .unwrap();
    let mut row = connection.query("SELECT * FROM text_array_grid_edit").await.unwrap();
    let id_index = row.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = row.columns.iter().position(|column| column.name == "value").unwrap();
    row.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "text_array_grid_edit",
        &row.columns,
        &[(value_index, Value::Text(TEXT_ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let elements = connection
        .query("SELECT ordinality, item IS NULL, item FROM text_array_grid_edit CROSS JOIN LATERAL unnest(text_array_grid_edit.value) WITH ORDINALITY AS element(item, ordinality) ORDER BY ordinality")
        .await
        .unwrap();
    assert_eq!(
        elements.rows,
        vec![
            vec![Value::Int(1), Value::Bool(false), Value::Text("plain".into())],
            vec![Value::Int(2), Value::Bool(true), Value::Null],
            vec![
                Value::Int(3),
                Value::Bool(false),
                Value::Text("quote \" slash \\, comma".into()),
            ],
        ]
    );

    assert_bool_array_grid_edit(connection).await;
    assert_bytea_array_grid_edit(connection).await;
    assert_uuid_array_grid_edit(connection).await;
    assert_timestamptz_array_grid_edit(connection).await;
}

async fn assert_bool_array_grid_edit(connection: &dyn Connection) {
    connection
        .execute("CREATE TABLE bool_array_grid_edit (id integer PRIMARY KEY, value boolean[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO bool_array_grid_edit VALUES (1, ARRAY[true]::boolean[]), (2, ARRAY[false]::boolean[])")
        .await
        .unwrap();
    let mut rows = connection
        .query("SELECT * FROM bool_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    let sibling_wire = connection
        .query("SELECT encode(array_send(value), 'hex') FROM bool_array_grid_edit WHERE id = 2")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let id_index = rows.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = rows.columns.iter().position(|column| column.name == "value").unwrap();
    rows.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "bool_array_grid_edit",
        &rows.columns,
        &[(value_index, Value::Text("{true,false,NULL}".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let expected = connection
        .query("SELECT ARRAY[true,false,NULL]::boolean[]::text, encode(array_send(ARRAY[true,false,NULL]::boolean[]), 'hex')")
        .await
        .unwrap();
    let actual = connection
        .query("SELECT id, value::text, encode(array_send(value), 'hex') FROM bool_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    assert_eq!(actual.rows[0][0], Value::Int(1));
    assert_eq!(actual.rows[0][1], expected.rows[0][0]);
    assert_eq!(actual.rows[0][2], expected.rows[0][1]);
    assert_eq!(actual.rows[1][0], Value::Int(2));
    assert_eq!(actual.rows[1][2], sibling_wire);
}

async fn assert_bytea_array_grid_edit(connection: &dyn Connection) {
    const ARRAY: &str = r#"{"\\x00ff","\\x5c5c","",NULL}"#;
    const ORACLE: &str = "ARRAY[decode('00ff','hex'),decode('5c5c','hex'),decode('','hex'),NULL]::bytea[]";

    connection
        .execute("CREATE TABLE bytea_array_grid_edit (id integer PRIMARY KEY, value bytea[])")
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO bytea_array_grid_edit VALUES (1, ARRAY[decode('01','hex')]), (2, ARRAY[decode('1234','hex')])",
        )
        .await
        .unwrap();
    let mut rows = connection
        .query("SELECT * FROM bytea_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    let sibling_wire = connection
        .query("SELECT encode(array_send(value), 'hex') FROM bytea_array_grid_edit WHERE id = 2")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let id_index = rows.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = rows.columns.iter().position(|column| column.name == "value").unwrap();
    rows.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "bytea_array_grid_edit",
        &rows.columns,
        &[(value_index, Value::Text(ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let expected = connection
        .query(&format!("SELECT encode(array_send({ORACLE}), 'hex')"))
        .await
        .unwrap();
    let actual = connection
        .query("SELECT id, encode(array_send(value), 'hex') FROM bytea_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    assert_eq!(actual.rows[0][0], Value::Int(1));
    assert_eq!(actual.rows[0][1], expected.rows[0][0]);
    assert_eq!(actual.rows[1][0], Value::Int(2));
    assert_eq!(actual.rows[1][1], sibling_wire);
}

async fn assert_uuid_array_grid_edit(connection: &dyn Connection) {
    const ARRAY: &str = "{550e8400-e29b-41d4-a716-446655440000,6ba7b810-9dad-11d1-80b4-00c04fd430c8,NULL}";
    const ORACLE: &str =
        "ARRAY['550e8400-e29b-41d4-a716-446655440000'::uuid,'6ba7b810-9dad-11d1-80b4-00c04fd430c8'::uuid,NULL]::uuid[]";

    connection
        .execute("CREATE TABLE uuid_array_grid_edit (id integer PRIMARY KEY, value uuid[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO uuid_array_grid_edit VALUES (1, ARRAY['00000000-0000-0000-0000-000000000001'::uuid]), (2, ARRAY['00000000-0000-0000-0000-000000000002'::uuid])")
        .await
        .unwrap();
    let mut rows = connection
        .query("SELECT * FROM uuid_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    let sibling_wire = connection
        .query("SELECT encode(array_send(value), 'hex') FROM uuid_array_grid_edit WHERE id = 2")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let id_index = rows.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = rows.columns.iter().position(|column| column.name == "value").unwrap();
    rows.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "uuid_array_grid_edit",
        &rows.columns,
        &[(value_index, Value::Text(ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let expected = connection
        .query(&format!("SELECT encode(array_send({ORACLE}), 'hex')"))
        .await
        .unwrap();
    let actual = connection
        .query("SELECT id, encode(array_send(value), 'hex') FROM uuid_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    assert_eq!(actual.rows[0][0], Value::Int(1));
    assert_eq!(actual.rows[0][1], expected.rows[0][0]);
    assert_eq!(actual.rows[1][0], Value::Int(2));
    assert_eq!(actual.rows[1][1], sibling_wire);
}

async fn assert_timestamptz_array_grid_edit(connection: &dyn Connection) {
    const ARRAY: &str = r#"{"2026-09-30 12:34:56.123456+05:30","1999-12-31 23:59:59.000001-07:00",NULL}"#;
    const ORACLE: &str = "ARRAY['2026-09-30 12:34:56.123456+05:30'::timestamptz,'1999-12-31 23:59:59.000001-07:00'::timestamptz,NULL]::timestamptz[]";

    connection
        .execute("CREATE TABLE timestamptz_array_grid_edit (id integer PRIMARY KEY, value timestamptz[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO timestamptz_array_grid_edit VALUES (1, ARRAY['2000-01-01 00:00:00+00'::timestamptz]), (2, ARRAY['2001-01-01 00:00:00+00'::timestamptz])")
        .await
        .unwrap();
    let mut rows = connection
        .query("SELECT * FROM timestamptz_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    let sibling_wire = connection
        .query("SELECT encode(array_send(value), 'hex') FROM timestamptz_array_grid_edit WHERE id = 2")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let id_index = rows.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = rows.columns.iter().position(|column| column.name == "value").unwrap();
    rows.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "timestamptz_array_grid_edit",
        &rows.columns,
        &[(value_index, Value::Text(ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let expected = connection
        .query(&format!("SELECT encode(array_send({ORACLE}), 'hex')"))
        .await
        .unwrap();
    let actual = connection
        .query("SELECT id, encode(array_send(value), 'hex') FROM timestamptz_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    assert_eq!(actual.rows[0][0], Value::Int(1));
    assert_eq!(actual.rows[0][1], expected.rows[0][0]);
    assert_eq!(actual.rows[1][0], Value::Int(2));
    assert_eq!(actual.rows[1][1], sibling_wire);
}

pub async fn assert_float8_array_grid_edit(connection: &dyn Connection) {
    const ARRAY: &str = "{1.0000000000000002,-0,5e-324,NaN,Infinity,-Infinity,NULL}";
    const ORACLE: &str = "ARRAY[1.0000000000000002::float8, '-0'::float8, '5e-324'::float8, \
        'NaN'::float8, 'Infinity'::float8, '-Infinity'::float8, NULL::float8]";

    connection
        .execute("CREATE TABLE float8_array_grid_edit (id integer PRIMARY KEY, value float8[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO float8_array_grid_edit VALUES (1, ARRAY[0]::float8[]), (2, ARRAY[77]::float8[]), (3, ARRAY[88]::float8[])")
        .await
        .unwrap();
    let mut row = connection
        .query("SELECT * FROM float8_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    let id_index = row.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = row.columns.iter().position(|column| column.name == "value").unwrap();
    row.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "float8_array_grid_edit",
        &row.columns,
        &[(value_index, Value::Text(ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let expected = connection
        .query(&format!(
            "SELECT ({ORACLE})::text, encode(array_send(({ORACLE})::float8[]), 'hex')"
        ))
        .await
        .unwrap();
    let actual = connection
        .query(
            "SELECT id, value, value::text, encode(array_send(value), 'hex') \
             FROM float8_array_grid_edit ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(actual.rows[0][0], Value::Int(1));
    let Value::Text(native_text) = &expected.rows[0][0] else {
        panic!("native float8[] text oracle must be text: {:?}", expected.rows[0][0]);
    };
    let Value::Text(decoded_text) = &actual.rows[0][1] else {
        panic!("float8[] result must remain exact text: {:?}", actual.rows[0][1]);
    };
    assert_eq!(&actual.rows[0][2..], expected.rows[0].as_slice());
    assert_eq!(
        actual.rows[1][0],
        Value::Int(2),
        "the sibling row identity remains intact"
    );
    assert!(matches!(actual.rows[1][1], Value::Text(_)));

    assert_float8_array_csv_round_trip(connection, &actual, native_text, decoded_text).await;
}

async fn assert_float8_array_csv_round_trip(
    connection: &dyn Connection,
    result: &QueryResult,
    native_text: &str,
    decoded_text: &str,
) {
    let columns = vec![result.columns[0].clone(), result.columns[1].clone()];
    let row = vec![result.rows[0][0].clone(), result.rows[0][1].clone()];
    let csv_text = tablepro_core::export::render_csv(
        &columns,
        std::slice::from_ref(&row),
        &tablepro_core::export::CsvOptions::default(),
    );
    let mut reader = csv::Reader::from_reader(csv_text.as_bytes());
    assert_eq!(reader.headers().unwrap().iter().collect::<Vec<_>>(), ["id", "value"]);
    let exported = reader.records().next().unwrap().unwrap();
    assert_eq!(
        &exported[1], decoded_text,
        "CSV must preserve the driver's exact float8[] value text"
    );
    assert!(reader.records().next().is_none());

    let options = tablepro_core::import::CsvImportOptions::default();
    let sheet = tablepro_core::import::read_csv(csv_text.as_bytes(), &options, None).unwrap();
    let imported =
        tablepro_core::import::row_to_values(&sheet.rows[0], &[Some(0), Some(1)], &columns, &options, 2).unwrap();
    assert_eq!(imported, row, "CSV import must retain the array as exact text");

    connection
        .execute("CREATE TABLE float8_array_csv_import (id integer PRIMARY KEY, value float8[])")
        .await
        .unwrap();
    let import_columns = connection.fetch_columns(None, "float8_array_csv_import").await.unwrap();
    let import_mapping = [Some(0), Some(1)];
    let mut import_row = row.clone();
    import_row[0] = Value::Int(4);
    let import_csv = tablepro_core::export::render_csv(
        &columns,
        std::slice::from_ref(&import_row),
        &tablepro_core::export::CsvOptions::default(),
    );
    let import_sheet = tablepro_core::import::read_csv(import_csv.as_bytes(), &options, None).unwrap();
    let insert_plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "postgres",
            schema: None,
            table: "float8_array_csv_import",
            columns: &import_columns,
            mapping: &import_mapping,
        },
        &import_sheet,
        &options,
    )
    .expect("float8[] CSV should build an import plan");
    connection
        .execute_params(&insert_plan.statement, &insert_plan.rows[0])
        .await
        .expect("typed float8[] CSV import must cast array text to the native column type");
    let csv_restored = connection
        .query("SELECT id, value::text, encode(array_send(value), 'hex') FROM float8_array_csv_import")
        .await
        .unwrap();
    assert_eq!(
        csv_restored.rows,
        vec![vec![
            Value::Int(4),
            Value::Text(native_text.into()),
            result.rows[0][3].clone(),
        ]],
        "CSV import must preserve PostgreSQL array text and native wire bytes"
    );

    let mut update_columns = columns;
    update_columns[0].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "float8_array_grid_edit",
        &update_columns,
        &[(1, imported[1].clone())],
        &[Value::Int(2)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);
    let restored = connection
        .query("SELECT id, value::text, encode(array_send(value), 'hex') FROM float8_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    assert_eq!(restored.rows[0][0], Value::Int(1));
    assert_eq!(restored.rows[1][0], Value::Int(2));
    assert_eq!(restored.rows[1][1], Value::Text(native_text.into()));
    assert_eq!(restored.rows[1][2], restored.rows[0][2]);
    assert_eq!(restored.rows[2][0], Value::Int(3));
    assert_eq!(restored.rows[2][2], result.rows[2][3]);
}

async fn assert_array_csv_insert_contract(connection: &dyn Connection) {
    let cases = [
        ("bool[]", "ARRAY[true,false,NULL]::bool[]"),
        ("bytea[]", "ARRAY[decode('00ff275c','hex'),decode('','hex'),NULL]"),
        ("name[]", "ARRAY['alpha','','',NULL]::name[]"),
        ("int2[]", "ARRAY[-32768,0,32767]::int2[]"),
        ("int4[]", "ARRAY[-2147483648,0,2147483647]::int4[]"),
        ("int8[]", "ARRAY[-9223372036854775808,0,9223372036854775807]::int8[]"),
        ("oid[]", "ARRAY[0,4294967295,NULL]::oid[]"),
        ("text[]", "ARRAY[NULL,'NULL','','a,b','漢字 😀',E'line\\nnext']::text[]"),
        (
            "float4[]",
            "ARRAY[0.1,'-0','1e-45','NaN','Infinity','-Infinity',NULL]::float4[]",
        ),
        (
            "float8[]",
            "ARRAY[1e-200,1.0000000000000002,'-0','5e-324','NaN','Infinity','-Infinity',NULL]::float8[]",
        ),
        ("varchar[]", "ARRAY['x','',NULL]::varchar[]"),
        ("char(3)[]", "ARRAY['x',' y',NULL]::char(3)[]"),
        (
            "numeric[]",
            "ARRAY[1234567890123456789012345678901234567890,1.2300,'NaN'::numeric,'Infinity'::numeric,NULL]",
        ),
        ("uuid[]", "ARRAY['12345678-1234-5678-90ab-1234567890ab',NULL]::uuid[]"),
        (
            "date[]",
            "ARRAY['0002-12-31 BC','10000-01-01','infinity','-infinity',NULL]::date[]",
        ),
        ("time[]", "ARRAY['00:00:00','24:00:00','23:59:59.999999',NULL]::time[]"),
        (
            "timetz[]",
            "ARRAY['00:00:00+15:59:59','24:00:00-15:59:59',NULL]::timetz[]",
        ),
        (
            "timestamp[]",
            "ARRAY['0001-01-01 00:00:00.000001 BC','10000-01-01 23:59:59.999999','infinity','-infinity',NULL]::timestamp[]",
        ),
        (
            "timestamptz[]",
            "ARRAY['2024-11-03 01:30:00-04','2024-11-03 01:30:00-05','infinity','-infinity',NULL]::timestamptz[]",
        ),
        (
            "interval[]",
            "ARRAY['-1 month +2 days +0.000001 seconds','+1 month -2 days -0.000001 seconds',NULL]::interval[]",
        ),
    ];
    let definitions = cases
        .iter()
        .enumerate()
        .map(|(index, (kind, _))| format!("value_{index} {kind}"))
        .collect::<Vec<_>>()
        .join(", ");
    let expressions = cases
        .iter()
        .enumerate()
        .map(|(index, (_, expression))| format!("({expression}) AS value_{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    connection
        .execute(&format!("CREATE TABLE array_csv_source AS SELECT {expressions}"))
        .await
        .unwrap();
    connection
        .execute(&format!("CREATE TABLE array_csv_target ({definitions})"))
        .await
        .unwrap();

    let source = connection.query("SELECT * FROM array_csv_source").await.unwrap();
    let wire_columns = cases
        .iter()
        .enumerate()
        .map(|(index, _)| format!("encode(array_send(value_{index}), 'hex')"))
        .collect::<Vec<_>>()
        .join(", ");
    let source_wire = connection
        .query(&format!("SELECT {wire_columns} FROM array_csv_source"))
        .await
        .unwrap();
    let csv_text = tablepro_core::export::render_csv(
        &source.columns,
        &source.rows,
        &tablepro_core::export::CsvOptions::default(),
    );
    let options = tablepro_core::import::CsvImportOptions::default();
    let sheet = tablepro_core::import::read_csv(csv_text.as_bytes(), &options, None).unwrap();
    let columns = connection.fetch_columns(None, "array_csv_target").await.unwrap();
    let mapping = (0..columns.len()).map(Some).collect::<Vec<_>>();
    let plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "postgres",
            schema: None,
            table: "array_csv_target",
            columns: &columns,
            mapping: &mapping,
        },
        &sheet,
        &options,
    )
    .expect("every allowlisted built-in array should produce a typed CSV INSERT plan");
    assert_eq!(plan.rows, source.rows, "array CSV parsing preserves exact text cells");
    connection
        .execute_params(&plan.statement, &plan.rows[0])
        .await
        .expect("typed PostgreSQL array CSV INSERT must cast every text cell to its array type");

    let target_wire = connection
        .query(&format!("SELECT {wire_columns} FROM array_csv_target"))
        .await
        .unwrap();
    let target = connection.query("SELECT * FROM array_csv_target").await.unwrap();
    assert_eq!(
        target_wire.rows, source_wire.rows,
        "native array_send bytes after CSV INSERT"
    );
    assert_eq!(target.rows, source.rows, "native array text after CSV INSERT");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_builtin_array_families_survive_typed_csv_insert() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    assert_array_csv_insert_contract(connection.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_float8_array_grid_edit_preserves_special_and_adjacent_values() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    assert_float8_array_grid_edit(connection.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_array_grid_edit_preserves_array_elements() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    assert_array_grid_edit(connection.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_numeric_array_grid_edit_preserves_elements() {
    const NUMERIC_ARRAY: &str =
        "{1234567890123456789012345678901234567890.12345678901234567890,1.2300,NaN,Infinity,-Infinity,NULL}";
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE TABLE numeric_array_grid_edit (id integer PRIMARY KEY, value numeric[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO numeric_array_grid_edit VALUES (1, ARRAY[0]::numeric[])")
        .await
        .unwrap();
    let mut row = connection.query("SELECT * FROM numeric_array_grid_edit").await.unwrap();
    let id_index = row.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = row.columns.iter().position(|column| column.name == "value").unwrap();
    row.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "numeric_array_grid_edit",
        &row.columns,
        &[(value_index, Value::Text(NUMERIC_ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let elements = connection
        .query("SELECT ordinality, item IS NULL, item::text FROM numeric_array_grid_edit CROSS JOIN LATERAL unnest(numeric_array_grid_edit.value) WITH ORDINALITY AS element(item, ordinality) ORDER BY ordinality")
        .await
        .unwrap();
    assert_eq!(
        elements.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Bool(false),
                Value::Text("1234567890123456789012345678901234567890.12345678901234567890".into()),
            ],
            vec![Value::Int(2), Value::Bool(false), Value::Text("1.2300".into())],
            vec![Value::Int(3), Value::Bool(false), Value::Text("NaN".into())],
            vec![Value::Int(4), Value::Bool(false), Value::Text("Infinity".into())],
            vec![Value::Int(5), Value::Bool(false), Value::Text("-Infinity".into())],
            vec![Value::Int(6), Value::Bool(true), Value::Null],
        ]
    );
}

fn assert_array_value(kind: &str, result: &QueryResult) {
    let value = &result.rows[0][0];
    assert!(matches!(value, Value::Null | Value::Text(_)), "{kind}: {value:?}");
    let expected_type = if kind == "bpchar[]" {
        "CHAR[]".into()
    } else {
        kind.to_ascii_uppercase()
    };
    assert_eq!(result.columns[0].data_type, expected_type);
    let json = tablepro_core::export::row_to_json(&result.columns, &result.rows[0]);
    match value {
        Value::Null => assert!(json["value"].is_null()),
        Value::Text(text) => assert_eq!(json["value"].as_str(), Some(text.as_str())),
        other => panic!("unexpected array value: {other:?}"),
    }
}
