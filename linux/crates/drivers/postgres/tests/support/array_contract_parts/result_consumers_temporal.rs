#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_timestamp_array_file_consumers_preserve_values_across_dmy_datestyle() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let mut transaction = connection.begin().await.unwrap();
    transaction.execute("SET LOCAL DateStyle = 'SQL, DMY'").await.unwrap();
    let expression = "ARRAY['2024-12-31 23:59:59.123456'::timestamp, \
                      '2001-02-03 04:05:06'::timestamp, \
                      '0002-12-31 23:59:59 BC'::timestamp, \
                      '10000-01-01 00:00:00'::timestamp, \
                      'infinity'::timestamp, '-infinity'::timestamp, NULL]";
    let result = transaction
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("timestamp[]", &result);

    let native = transaction
        .query(&format!(
            "SELECT current_setting('DateStyle'), pg_typeof({expression})::text, \
                    {expression}::text, array_to_json({expression})::text, \
                    encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(native.rows[0][0], Value::Text("SQL, DMY".into()));
    assert_eq!(native.rows[0][1], Value::Text("timestamp without time zone[]".into()));
    assert_ne!(result.rows[0][0], native.rows[0][2]);
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("timestamp[] result must remain exact array text: {:?}", result.rows[0][0]);
    };
    assert!(driver_text.contains("2024-12-31 23:59:59.123456"), "{driver_text}");
    assert!(driver_text.contains("0002-12-31 23:59:59 BC"), "{driver_text}");
    assert!(driver_text.contains("10000-01-01 00:00:00"), "{driver_text}");
    assert!(driver_text.contains("infinity"), "{driver_text}");
    assert!(driver_text.contains("-infinity"), "{driver_text}");

    transaction.execute("SET LOCAL DateStyle = 'ISO, MDY'").await.unwrap();
    let rebound = transaction
        .query_params(
            "SELECT array_to_json($1::text::timestamp[])::text, \
                    encode(array_send($1::text::timestamp[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], native.rows[0][3]);
    assert_eq!(rebound.rows[0][1], native.rows[0][4]);

    let directory = tempfile::tempdir().unwrap();
    let csv_options = tablepro_core::export::CsvOptions::default();
    let csv_path = directory.path().join("timestamp-array-dmy.csv");
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

    transaction
        .execute("CREATE TABLE timestamp_array_dmy_target (value timestamp[])")
        .await
        .unwrap();
    transaction
        .execute(
            "INSERT INTO timestamp_array_dmy_target \
             VALUES (ARRAY['1999-01-01 00:00:00'::timestamp])",
        )
        .await
        .unwrap();
    let sibling = transaction
        .query(
            "SELECT pg_typeof(value)::text, array_to_json(value)::text, \
                    encode(array_send(value), 'hex') FROM timestamp_array_dmy_target",
        )
        .await
        .unwrap();
    let sql_path = directory.path().join("timestamp-array-dmy.sql");
    tablepro_core::export::write_result_file(
        &sql_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Sql,
            csv: &csv_options,
            sql: Some(tablepro_core::export::SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "timestamp_array_dmy_target",
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
            "SELECT pg_typeof(value)::text, array_to_json(value)::text, \
                    encode(array_send(value), 'hex') FROM timestamp_array_dmy_target",
        )
        .await
        .unwrap();
    assert_eq!(restored.rows.len(), 2);
    assert!(restored.rows.contains(&sibling.rows[0]));
    assert!(
        restored.rows.contains(&vec![
            native.rows[0][1].clone(),
            native.rows[0][3].clone(),
            native.rows[0][4].clone(),
        ]),
        "restored {:?}, expected native type, JSON and wire values",
        restored.rows
    );
    transaction.rollback().await.unwrap();
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
async fn value_contract_timestamptz_array_file_exports_preserve_instants_under_non_utc_session() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute("SET LOCAL TIME ZONE 'America/New_York'")
        .await
        .unwrap();
    let expression = "ARRAY['2024-11-03 01:30:00.123456-04'::timestamptz, \
        '2024-11-03 01:30:00.123456-05'::timestamptz, \
        '0001-01-01 12:34:56+00 BC'::timestamptz, \
        'infinity'::timestamptz, '-infinity'::timestamptz, NULL]";
    let result = transaction
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_array_value("timestamptz[]", &result);

    let oracle = transaction
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
    assert!(driver_text.contains("+00:00"), "{driver_text}");
    assert_ne!(result.rows[0][0], oracle.rows[0][1]);
    let Value::Text(native_json) = &oracle.rows[0][2] else {
        panic!("timestamptz[] array_to_json returned {:?}", oracle.rows[0][2]);
    };
    let json_elements = serde_json::from_str::<serde_json::Value>(native_json).unwrap();
    assert!(json_elements[0].as_str().unwrap().ends_with("-04:00"));
    assert!(json_elements[1].as_str().unwrap().ends_with("-05:00"));
    assert_ne!(json_elements[0], json_elements[1]);

    let rebound = transaction
        .query_params(
            "SELECT encode(array_send($1::text::timestamptz[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], oracle.rows[0][3]);

    let directory = tempfile::tempdir().unwrap();
    let csv_options = tablepro_core::export::CsvOptions::default();
    let json_path = directory.path().join("timestamptz-array-non-utc.json");
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

    let csv_path = directory.path().join("timestamptz-array-non-utc.csv");
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

    let xlsx_path = directory.path().join("timestamptz-array-non-utc.xlsx");
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

    transaction
        .execute("CREATE TABLE timestamptz_array_non_utc_target (value timestamptz[])")
        .await
        .unwrap();
    let sql_path = directory.path().join("timestamptz-array-non-utc.sql");
    tablepro_core::export::write_result_file(
        &sql_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Sql,
            csv: &csv_options,
            sql: Some(tablepro_core::export::SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "timestamptz_array_non_utc_target",
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
    transaction
        .execute("SET LOCAL TIME ZONE 'Asia/Kathmandu'")
        .await
        .unwrap();
    let restored = transaction
        .query(
            "SELECT encode(array_send(value), 'hex') FROM timestamptz_array_non_utc_target",
        )
        .await
        .unwrap();
    assert_eq!(restored.rows[0][0], oracle.rows[0][3]);
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
async fn value_contract_interval_array_csv_and_sql_replay_survive_intervalstyle_change() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute("SET LOCAL IntervalStyle = 'postgres_verbose'")
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
    let native = transaction
        .query(&format!(
            "SELECT pg_typeof({expression})::text, encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(native.rows[0][0], Value::Text("interval[]".into()));
    let Value::Text(driver_text) = &result.rows[0][0] else {
        panic!("interval[] result must remain exact array text: {:?}", result.rows[0][0]);
    };

    let directory = tempfile::tempdir().unwrap();
    let csv_options = tablepro_core::export::CsvOptions::default();
    let sql_path = directory.path().join("interval-array-style-transition.sql");
    tablepro_core::export::write_result_file(
        &sql_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Sql,
            csv: &csv_options,
            sql: Some(tablepro_core::export::SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "interval_style_transition_target",
            }),
        },
        || false,
        |_| {},
    )
    .unwrap();
    transaction
        .execute("CREATE TABLE interval_style_transition_target (value interval[])")
        .await
        .unwrap();

    let csv_path = directory.path().join("interval-array-style-transition.csv");
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
    let csv_value = csv.records().next().unwrap().unwrap()[0].to_owned();
    assert_eq!(csv_value, *driver_text);

    transaction.execute("SET LOCAL IntervalStyle = 'iso_8601'").await.unwrap();
    let setting = transaction.query("SELECT current_setting('IntervalStyle')").await.unwrap();
    assert_eq!(setting.rows[0][0], Value::Text("iso_8601".into()));
    transaction
        .execute(&std::fs::read_to_string(sql_path).unwrap())
        .await
        .unwrap();
    let rebound = transaction
        .query_params(
            "SELECT encode(array_send($1::text::interval[]), 'hex')",
            std::slice::from_ref(&Value::Text(driver_text.clone())),
        )
        .await
        .unwrap();
    let csv_rebound = transaction
        .query_params(
            "SELECT encode(array_send($1::text::interval[]), 'hex')",
            std::slice::from_ref(&Value::Text(csv_value)),
        )
        .await
        .unwrap();
    let restored = transaction
        .query("SELECT encode(array_send(value), 'hex') FROM interval_style_transition_target")
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], native.rows[0][1]);
    assert_eq!(csv_rebound.rows[0][0], native.rows[0][1]);
    assert_eq!(restored.rows[0][0], native.rows[0][1]);
    transaction.rollback().await.unwrap();
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
