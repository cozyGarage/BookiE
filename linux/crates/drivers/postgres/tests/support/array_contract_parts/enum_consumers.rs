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
async fn value_contract_custom_enum_array_preserves_boundary_whitespace_labels() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute(
            "CREATE TYPE value_contract_whitespace_enum AS ENUM \
             (' leading', 'trailing ', ' both ', 'NULL', '', 'sibling')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_whitespace_target \
             (id integer PRIMARY KEY, labels value_contract_whitespace_enum[])",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_whitespace_target VALUES \
             (99, ARRAY['sibling'::value_contract_whitespace_enum])",
        )
        .await
        .unwrap();
    let sibling_wire = connection
        .query(
            "SELECT encode(array_send(labels), 'hex') \
             FROM value_contract_whitespace_target WHERE id = 99",
        )
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let expression = "ARRAY[' leading'::value_contract_whitespace_enum, \
        'trailing '::value_contract_whitespace_enum, \
        ' both '::value_contract_whitespace_enum, \
        'NULL'::value_contract_whitespace_enum, \
        ''::value_contract_whitespace_enum, NULL]";
    let source = connection
        .query(&format!("SELECT 7 AS id, {expression} AS labels"))
        .await
        .unwrap();
    assert_eq!(source.columns[1].data_type, "value_contract_whitespace_enum[]");
    assert_eq!(
        source.rows[0][1],
        Value::Text(r#"{" leading","trailing "," both ","NULL","",NULL}"#.into())
    );

    let native = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, array_to_json({expression})::text, \
                    encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(native.rows[0][0], Value::Text("value_contract_whitespace_enum[]".into()));
    let Value::Text(native_json) = &native.rows[0][1] else {
        panic!("native whitespace enum array JSON returned {:?}", native.rows[0][1]);
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(native_json).unwrap(),
        serde_json::json!([" leading", "trailing ", " both ", "NULL", "", null])
    );

    let csv = tablepro_core::export::render_csv(
        &source.columns,
        &source.rows,
        &tablepro_core::export::CsvOptions::default(),
    );
    let options = tablepro_core::import::CsvImportOptions::default();
    let sheet = tablepro_core::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let mapping = [Some(0), Some(1)];
    let imported = tablepro_core::import::row_to_values(&sheet.rows[0], &mapping, &source.columns, &options, 2).unwrap();
    assert_eq!(imported, source.rows[0]);

    let rebound = connection
        .query_params(
            "SELECT pg_typeof($1::text::value_contract_whitespace_enum[])::text, \
                    array_to_json($1::text::value_contract_whitespace_enum[])::text, \
                    encode(array_send($1::text::value_contract_whitespace_enum[]), 'hex')",
            std::slice::from_ref(&imported[1]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], native.rows[0][0]);
    assert_eq!(rebound.rows[0][1], native.rows[0][1]);
    assert_eq!(rebound.rows[0][2], native.rows[0][2]);

    let columns = connection
        .fetch_columns(None, "value_contract_whitespace_target")
        .await
        .unwrap();
    let plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "postgres",
            schema: None,
            table: "value_contract_whitespace_target",
            columns: &columns,
            mapping: &mapping,
        },
        &sheet,
        &options,
    )
    .unwrap();
    connection.execute_params(&plan.statement, &plan.rows[0]).await.unwrap();
    let restored = connection
        .query(
            "SELECT id, pg_typeof(labels)::text, labels::text, array_to_json(labels)::text, \
                    encode(array_send(labels), 'hex') \
             FROM value_contract_whitespace_target ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(restored.rows.len(), 2);
    assert_eq!(restored.rows[0][0], Value::Int(7));
    assert_eq!(restored.rows[0][1], native.rows[0][0]);
    assert_eq!(restored.rows[0][3], native.rows[0][1]);
    assert_eq!(restored.rows[0][4], native.rows[0][2]);
    assert_eq!(restored.rows[1][0], Value::Int(99));
    assert_eq!(restored.rows[1][4], sibling_wire);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_accepts_maximum_utf8_label_length() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let maximum_label = "漢".repeat(21);
    assert_eq!(maximum_label.len(), 63);
    connection
        .execute(&format!(
            "CREATE TYPE value_contract_max_enum_label AS ENUM ('{maximum_label}')"
        ))
        .await
        .unwrap();

    let overlong_label = format!("{maximum_label}x");
    assert_eq!(overlong_label.len(), 64);
    let rejected = connection
        .execute(&format!(
            "CREATE TYPE value_contract_overlong_enum_label AS ENUM ('{overlong_label}')"
        ))
        .await;
    assert!(
        matches!(
            rejected,
            Err(tablepro_core::DriverError::Query {
                sqlstate: Some(ref code),
                ..
            }) if code == "42602"
        ),
        "PostgreSQL must reject a 64-byte enum label without truncating it: {rejected:?}"
    );

    let expression = format!(
        "ARRAY['{maximum_label}'::value_contract_max_enum_label]"
    );
    let result = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .unwrap();
    assert_eq!(result.columns[0].data_type, "value_contract_max_enum_label[]");

    let native = connection
        .query(&format!(
            "SELECT pg_typeof({expression})::text, array_to_json({expression})::text, \
                    encode(array_send({expression}), 'hex')"
        ))
        .await
        .unwrap();
    assert_eq!(native.rows[0][0], Value::Text("value_contract_max_enum_label[]".into()));
    let Value::Text(array_text) = &result.rows[0][0] else {
        panic!("maximum-label enum array must remain text: {:?}", result.rows[0][0]);
    };
    assert!(array_text.contains(&maximum_label));
    assert_eq!(
        native.rows[0][1],
        Value::Text(serde_json::json!([maximum_label]).to_string())
    );

    let rebound = connection
        .query_params(
            "SELECT pg_typeof($1::text::value_contract_max_enum_label[])::text, \
                    array_to_json($1::text::value_contract_max_enum_label[])::text, \
                    encode(array_send($1::text::value_contract_max_enum_label[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], native.rows[0][0]);
    assert_eq!(rebound.rows[0][1], native.rows[0][1]);
    assert_eq!(rebound.rows[0][2], native.rows[0][2]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_agg_preserves_order_and_nulls() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE TYPE value_contract_array_agg_enum AS ENUM ('NULL', '', '東京', 'a,b', 'o''brien')")
        .await
        .unwrap();

    let source = "VALUES \
        (1, 'NULL'::value_contract_array_agg_enum), \
        (2, ''::value_contract_array_agg_enum), \
        (3, '東京'::value_contract_array_agg_enum), \
        (4, 'a,b'::value_contract_array_agg_enum), \
        (5, 'o''brien'::value_contract_array_agg_enum), \
        (6, NULL::value_contract_array_agg_enum)";
    let aggregate = format!(
        "(SELECT array_agg(label ORDER BY ordinal) FROM ({source}) AS input(ordinal, label))"
    );
    let result = connection.query(&format!("SELECT {aggregate} AS value")).await.unwrap();
    assert_eq!(result.columns[0].data_type, "value_contract_array_agg_enum[]");
    let Value::Text(_) = &result.rows[0][0] else {
        panic!("custom enum array aggregate must remain textual: {:?}", result.rows[0][0]);
    };

    let oracle = connection
        .query(&format!(
            "SELECT pg_typeof(value)::text, array_to_json(value)::text, \
                    encode(array_send(value), 'hex') \
             FROM (SELECT {aggregate} AS value) AS native"
        ))
        .await
        .unwrap();
    assert_eq!(oracle.rows[0][0], Value::Text("value_contract_array_agg_enum[]".into()));
    let Value::Text(json_text) = &oracle.rows[0][1] else {
        panic!("array_to_json oracle returned {:?}", oracle.rows[0][1]);
    };
    let expected_json = serde_json::json!(["NULL", "", "東京", "a,b", "o'brien", null]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(json_text).unwrap(),
        expected_json
    );

    let rebound = connection
        .query_params(
            "SELECT array_to_json($1::text::value_contract_array_agg_enum[])::text, \
                    encode(array_send($1::text::value_contract_array_agg_enum[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    let Value::Text(rebound_json) = &rebound.rows[0][0] else {
        panic!("rebound array_to_json returned {:?}", rebound.rows[0][0]);
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(rebound_json).unwrap(),
        expected_json
    );
    assert_eq!(rebound.rows[0][1], oracle.rows[0][2]);

    let no_rows_aggregate = format!(
        "(SELECT array_agg(label ORDER BY ordinal) FILTER (WHERE false) \
         FROM ({source}) AS input(ordinal, label))"
    );
    let no_rows = connection
        .query(&format!(
            "SELECT {no_rows_aggregate} AS value, pg_typeof({no_rows_aggregate})::text AS array_type"
        ))
        .await
        .unwrap();
    assert_eq!(no_rows.columns[0].data_type, "value_contract_array_agg_enum[]");
    assert_eq!(no_rows.rows[0][0], Value::Null);
    assert_eq!(no_rows.rows[0][1], Value::Text("value_contract_array_agg_enum[]".into()));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_agg_distinct_filter_preserves_native_order_and_nulls() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute(
            "CREATE TYPE value_contract_array_agg_distinct_enum \
             AS ENUM ('NULL', '', '東京', 'a,b', 'o''brien')",
        )
        .await
        .unwrap();

    let aggregate = "array_agg(DISTINCT label ORDER BY label) FILTER (WHERE keep_it)";
    let source = "VALUES \
        (true, 'NULL'::value_contract_array_agg_distinct_enum), \
        (true, 'NULL'::value_contract_array_agg_distinct_enum), \
        (true, ''::value_contract_array_agg_distinct_enum), \
        (true, ''::value_contract_array_agg_distinct_enum), \
        (true, '東京'::value_contract_array_agg_distinct_enum), \
        (true, '東京'::value_contract_array_agg_distinct_enum), \
        (true, 'a,b'::value_contract_array_agg_distinct_enum), \
        (false, 'o''brien'::value_contract_array_agg_distinct_enum), \
        (true, NULL::value_contract_array_agg_distinct_enum), \
        (true, NULL::value_contract_array_agg_distinct_enum)";
    let query = format!(
        "WITH input(keep_it, label) AS ({source}) \
         SELECT {aggregate} AS value FROM input"
    );
    let result = connection.query(&query).await.unwrap();
    assert_eq!(result.columns[0].data_type, "value_contract_array_agg_distinct_enum[]");
    let Value::Text(_) = &result.rows[0][0] else {
        panic!("custom enum aggregate must remain textual: {:?}", result.rows[0][0]);
    };

    let native = connection
        .query(&format!(
            "WITH input(keep_it, label) AS ({source}) \
             SELECT pg_typeof(value)::text, array_to_json(value)::text, \
                    encode(array_send(value), 'hex') \
             FROM (SELECT {aggregate} AS value FROM input) AS aggregate_result"
        ))
        .await
        .unwrap();
    assert_eq!(
        native.rows[0][0],
        Value::Text("value_contract_array_agg_distinct_enum[]".into())
    );
    let Value::Text(json_text) = &native.rows[0][1] else {
        panic!("native enum aggregate JSON oracle returned {:?}", native.rows[0][1]);
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(json_text).unwrap(),
        serde_json::json!(["NULL", "", "東京", "a,b", null])
    );

    let rebound = connection
        .query_params(
            "SELECT pg_typeof($1::text::value_contract_array_agg_distinct_enum[])::text, \
                    array_to_json($1::text::value_contract_array_agg_distinct_enum[])::text, \
                    encode(array_send($1::text::value_contract_array_agg_distinct_enum[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0][0], native.rows[0][0]);
    assert_eq!(rebound.rows[0][1], native.rows[0][1]);
    assert_eq!(rebound.rows[0][2], native.rows[0][2]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_agg_keeps_target_type_under_shadowed_search_path() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE SCHEMA value_contract_agg_target")
        .await
        .unwrap();
    connection
        .execute("CREATE SCHEMA value_contract_agg_shadow")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_agg_target.status AS ENUM \
             ('NULL', '', 'shared', 'target-only', '東京')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_agg_shadow.status AS ENUM \
             ('shadow-only', 'shared', 'target-only')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_agg_target.rows \
             (ordinal INT PRIMARY KEY, label value_contract_agg_target.status)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_agg_target.rows VALUES \
             (1, 'target-only'), (2, 'NULL'), (3, 'shared'), \
             (4, ''), (5, NULL), (6, '東京')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "SET search_path TO value_contract_agg_shadow, \
             value_contract_agg_target, public",
        )
        .await
        .unwrap();

    let aggregate = "array_agg(DISTINCT label ORDER BY label)";
    let result = connection
        .query(&format!(
            "SELECT {aggregate} AS value \
             FROM value_contract_agg_target.rows"
        ))
        .await
        .unwrap();
    let Value::Text(_) = &result.rows[0][0] else {
        panic!("custom enum aggregate must remain textual: {:?}", result.rows[0][0]);
    };

    let native = connection
        .query(&format!(
            "SELECT pg_typeof(value)::text, \
                    pg_typeof(value)::oid = \
                        'value_contract_agg_target.status[]'::regtype::oid, \
                    array_to_json(value)::text, encode(array_send(value), 'hex') \
             FROM (SELECT {aggregate} AS value \
                   FROM value_contract_agg_target.rows) AS aggregate_result"
        ))
        .await
        .unwrap();
    assert_eq!(
        native.rows[0][0],
        Value::Text("value_contract_agg_target.status[]".into())
    );
    assert_eq!(native.rows[0][1], Value::Bool(true));
    assert_eq!(
        native.rows[0][2],
        Value::Text(r#"["NULL","","shared","target-only","東京",null]"#.into())
    );

    let rebound = connection
        .query_params(
            "SELECT pg_typeof($1::text::value_contract_agg_target.status[])::text, \
                    pg_typeof($1::text::value_contract_agg_target.status[])::oid = \
                        'value_contract_agg_target.status[]'::regtype::oid, \
                    array_to_json($1::text::value_contract_agg_target.status[])::text, \
                    encode(array_send($1::text::value_contract_agg_target.status[]), 'hex')",
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(rebound.rows[0], native.rows[0]);

    let shadow_only_label = Value::Text("{shadow-only}".into());
    assert!(
        connection
            .query_params(
                "SELECT $1::text::value_contract_agg_target.status[]",
                std::slice::from_ref(&shadow_only_label),
            )
            .await
            .is_err()
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
             ('NULL', '', '東京', 'a,b', 'a\"b', '<tag>&', '=1+1', 'slash\\path', 'sibling')",
        )
        .await
        .unwrap();
    let expression = "ARRAY['NULL'::value_contract_file_enum, \
        ''::value_contract_file_enum, '東京'::value_contract_file_enum, \
        'a,b'::value_contract_file_enum, 'a\"b'::value_contract_file_enum, \
        '<tag>&'::value_contract_file_enum, '=1+1'::value_contract_file_enum, \
        'slash\\path'::value_contract_file_enum, NULL]";
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
        serde_json::json!(["NULL", "", "東京", "a,b", "a\"b", "<tag>&", "=1+1", "slash\\path", null])
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

    let xlsx_path = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("enum-array.xlsx"));
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
    connection
        .execute(
            "INSERT INTO enum_array_filewriter_target \
             VALUES (ARRAY['sibling'::value_contract_file_enum])",
        )
        .await
        .unwrap();
    let sibling = connection
        .query(
            "SELECT value::text, array_to_json(value)::text, \
                    encode(array_send(value), 'hex') FROM enum_array_filewriter_target",
        )
        .await
        .unwrap();
    assert_eq!(sibling.rows.len(), 1);
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
    let sql_file = std::fs::read_to_string(sql_path).unwrap();
    for (standard_conforming_strings, backslash_quote) in [
        ("on", "safe_encoding"),
        ("on", "off"),
        ("off", "off"),
        ("off", "on"),
    ] {
        let mut transaction = connection.begin().await.unwrap();
        transaction
            .execute(&format!(
                "SET LOCAL standard_conforming_strings = {standard_conforming_strings}"
            ))
            .await
            .unwrap();
        transaction
            .execute(&format!("SET LOCAL backslash_quote = {backslash_quote}"))
            .await
            .unwrap();
        transaction.execute(&sql_file).await.unwrap();
        let restored = transaction
            .query(
                "SELECT value::text, array_to_json(value)::text, \
                        encode(array_send(value), 'hex') FROM enum_array_filewriter_target",
            )
            .await
            .unwrap();
        assert_eq!(
            restored.rows.len(),
            2,
            "enum[] SQL replay with standard_conforming_strings={standard_conforming_strings}, backslash_quote={backslash_quote}"
        );
        assert!(restored.rows.contains(&oracle.rows[0][1..].to_vec()));
        assert!(restored.rows.contains(&sibling.rows[0]));
        transaction.rollback().await.unwrap();
    }
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

    let csv_options = tablepro_core::export::CsvOptions::default();
    let csv = tablepro_core::export::render_csv(&source.columns, &source.rows, &csv_options);
    let import_options = tablepro_core::import::CsvImportOptions::default();
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

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_csv_import_uses_target_type_under_shadowed_search_path() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection.execute("CREATE SCHEMA enum_array_shadow_a").await.unwrap();
    connection.execute("CREATE SCHEMA enum_array_shadow_b").await.unwrap();
    connection
        .execute("CREATE TYPE enum_array_shadow_a.label AS ENUM ('ready')")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE enum_array_shadow_b.label AS ENUM ('ready', 'paused')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE enum_array_shadow_b.target \
             (id integer PRIMARY KEY, labels enum_array_shadow_b.label[])",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO enum_array_shadow_b.target VALUES \
             (2, ARRAY['ready'::enum_array_shadow_b.label])",
        )
        .await
        .unwrap();

    let source = connection
        .query("SELECT 1 AS id, ARRAY['paused'::enum_array_shadow_b.label] AS labels")
        .await
        .unwrap();
    let csv = tablepro_core::export::render_csv(
        &source.columns,
        &source.rows,
        &tablepro_core::export::CsvOptions::default(),
    );
    let import_options = tablepro_core::import::CsvImportOptions::default();
    let sheet = tablepro_core::import::read_csv(csv.as_bytes(), &import_options, None).unwrap();
    let columns = connection
        .fetch_columns(Some("enum_array_shadow_b"), "target")
        .await
        .unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "enum_array_shadow_b".into(),
            name: "label".into(),
        })
    );
    let mapping = [Some(0), Some(1)];
    let plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "postgres",
            schema: Some("enum_array_shadow_b"),
            table: "target",
            columns: &columns,
            mapping: &mapping,
        },
        &sheet,
        &import_options,
    )
    .unwrap();
    assert!(plan.statement.contains("$2::text::\"enum_array_shadow_b\".\"label\"[]"));

    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute("SET LOCAL search_path TO enum_array_shadow_a, public")
        .await
        .unwrap();
    assert_eq!(
        transaction.query("SELECT current_schema()::text").await.unwrap().rows,
        vec![vec![Value::Text("enum_array_shadow_a".into())]]
    );
    transaction
        .execute_params(&plan.statement, &plan.rows[0])
        .await
        .unwrap();
    let target = transaction
        .query(
            "SELECT id, pg_typeof(labels)::text, labels::text, array_to_json(labels)::text, \
                    encode(array_send(labels), 'hex') \
             FROM enum_array_shadow_b.target ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(target.rows.len(), 2);
    assert_eq!(target.rows[0][0], Value::Int(1));
    assert_eq!(target.rows[0][1], Value::Text("enum_array_shadow_b.label[]".into()));
    assert_eq!(target.rows[0][2], Value::Text("{paused}".into()));
    assert_eq!(target.rows[0][3], Value::Text("[\"paused\"]".into()));
    assert_eq!(target.rows[1][0], Value::Int(2));
    assert_eq!(target.rows[1][2], Value::Text("{ready}".into()));
    transaction.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_grid_edit_preserves_labels_and_siblings() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection.execute("CREATE SCHEMA enum_array_grid").await.unwrap();
    connection
        .execute("CREATE TYPE enum_array_grid.label AS ENUM ('NULL', '', '東京', 'a,b', 'sibling', 'ready')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE enum_array_grid.items \
             (id integer PRIMARY KEY, labels enum_array_grid.label[])",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO enum_array_grid.items VALUES \
             (1, ARRAY['sibling'::enum_array_grid.label]), \
             (2, ARRAY['ready'::enum_array_grid.label])",
        )
        .await
        .unwrap();
    let sibling_wire = connection
        .query("SELECT encode(array_send(labels), 'hex') FROM enum_array_grid.items WHERE id = 1")
        .await
        .unwrap()
        .rows[0][0]
        .clone();

    let mut columns = connection
        .fetch_columns(Some("enum_array_grid"), "items")
        .await
        .unwrap();
    let id_index = columns.iter().position(|column| column.name == "id").unwrap();
    let labels_index = columns.iter().position(|column| column.name == "labels").unwrap();
    assert_eq!(
        columns[labels_index].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "enum_array_grid".into(),
            name: "label".into(),
        })
    );
    columns[id_index].primary_key = true;
    let edited = r#"{"NULL","",東京,"a,b",NULL}"#;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some("enum_array_grid"),
        "items",
        &columns,
        &[(labels_index, Value::Text(edited.into()))],
        &[Value::Int(2)],
    )
    .unwrap();
    assert!(update.0.contains("$1::text::\"enum_array_grid\".\"label\"[]"));
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let native = connection
        .query(
            "SELECT pg_typeof(labels)::text, labels::text, array_dims(labels), \
                    array_to_json(labels)::text, encode(array_send(labels), 'hex') \
             FROM (SELECT ARRAY[\
                 'NULL'::enum_array_grid.label, ''::enum_array_grid.label, \
                 '東京'::enum_array_grid.label, 'a,b'::enum_array_grid.label, \
                 NULL::enum_array_grid.label\
             ] AS labels) AS source",
        )
        .await
        .unwrap();
    let saved = connection
        .query(
            "SELECT pg_typeof(labels)::text, labels::text, array_dims(labels), \
                    array_to_json(labels)::text, encode(array_send(labels), 'hex') \
             FROM enum_array_grid.items WHERE id = 2",
        )
        .await
        .unwrap();
    assert_eq!(saved.rows, native.rows);

    let invalid = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some("enum_array_grid"),
        "items",
        &columns,
        &[(labels_index, Value::Text("{not-a-label}".into()))],
        &[Value::Int(2)],
    )
    .unwrap();
    assert!(connection.execute_in_transaction(&[invalid]).await.is_err());
    let unchanged = connection
        .query(
            "SELECT pg_typeof(labels)::text, labels::text, array_dims(labels), \
                    array_to_json(labels)::text, encode(array_send(labels), 'hex') \
             FROM enum_array_grid.items WHERE id = 2",
        )
        .await
        .unwrap();
    assert_eq!(unchanged.rows, native.rows);

    let null_update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some("enum_array_grid"),
        "items",
        &columns,
        &[(labels_index, Value::Null)],
        &[Value::Int(2)],
    )
    .unwrap();
    assert!(null_update.0.contains("$1::text::\"enum_array_grid\".\"label\"[]"));
    connection.execute_in_transaction(&[null_update]).await.unwrap();
    let saved_null = connection
        .query(
            "SELECT labels IS NULL, pg_typeof(labels)::text, array_to_json(labels)::text, \
                    encode(array_send(labels), 'hex') \
             FROM enum_array_grid.items WHERE id = 2",
        )
        .await
        .unwrap();
    assert_eq!(
        saved_null.rows,
        vec![vec![
            Value::Bool(true),
            Value::Text("enum_array_grid.label[]".into()),
            Value::Null,
            Value::Null,
        ]]
    );

    let empty_update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some("enum_array_grid"),
        "items",
        &columns,
        &[(labels_index, Value::Text("{}".into()))],
        &[Value::Int(2)],
    )
    .unwrap();
    connection.execute_in_transaction(&[empty_update]).await.unwrap();
    let saved_empty = connection
        .query(
            "SELECT labels IS NULL, pg_typeof(labels)::text, labels::text, array_dims(labels), \
                    array_to_json(labels)::text, encode(array_send(labels), 'hex') \
             FROM enum_array_grid.items WHERE id = 2",
        )
        .await
        .unwrap();
    assert_eq!(saved_empty.rows[0][0], Value::Bool(false));
    assert_eq!(saved_empty.rows[0][1], Value::Text("enum_array_grid.label[]".into()));
    assert_eq!(saved_empty.rows[0][2], Value::Text("{}".into()));
    assert_eq!(saved_empty.rows[0][3], Value::Null);
    assert_eq!(saved_empty.rows[0][4], Value::Text("[]".into()));
    assert_ne!(saved_empty.rows[0][5], Value::Null);
    assert_eq!(
        connection
            .query("SELECT encode(array_send(labels), 'hex') FROM enum_array_grid.items WHERE id = 1")
            .await
            .unwrap()
            .rows[0][0],
        sibling_wire
    );
}
