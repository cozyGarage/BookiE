#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_array_preserves_labels_and_null() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE TYPE value_contract_domain_array_enum AS ENUM ('NULL', '', '東京', 'a,b', 'forbidden')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_domain_array_label AS value_contract_domain_array_enum \
             CHECK (VALUE <> 'forbidden'::value_contract_domain_array_enum)",
        )
        .await
        .unwrap();

    let result = connection
        .query(
            "SELECT labels, array_to_json(labels)::text AS native_values, pg_typeof(labels)::text AS native_type \
             FROM (VALUES (ARRAY['NULL'::value_contract_domain_array_label, \
             ''::value_contract_domain_array_label, '東京'::value_contract_domain_array_label, \
             'a,b'::value_contract_domain_array_label, NULL::value_contract_domain_array_label])) \
             AS arrays(labels)",
        )
        .await
        .unwrap();

    assert_eq!(
        result.rows,
        vec![vec![
            Value::Text("{\"NULL\",\"\",\"東京\",\"a,b\",NULL}".into()),
            Value::Text("[\"NULL\",\"\",\"東京\",\"a,b\",null]".into()),
            Value::Text("value_contract_domain_array_label[]".into()),
        ]]
    );

    let bound_array = connection
        .query_params(
            "SELECT $1::value_contract_domain_array_label[] AS labels, \
             array_to_json($1::value_contract_domain_array_label[])::text, \
             pg_typeof($1::value_contract_domain_array_label[])::text, \
             array_send($1::value_contract_domain_array_label[])",
            &[Value::Text(r#"{"NULL","","東京","a,b",NULL}"#.into())],
        )
        .await
        .unwrap();
    let native_array = connection
        .query(
            "SELECT labels, array_to_json(labels)::text, pg_typeof(labels)::text, array_send(labels) \
             FROM (SELECT ARRAY[\
                 'NULL'::value_contract_domain_array_label, \
                 ''::value_contract_domain_array_label, \
                 '東京'::value_contract_domain_array_label, \
                 'a,b'::value_contract_domain_array_label, \
                 NULL::value_contract_domain_array_label\
             ] AS labels) AS native",
        )
        .await
        .unwrap();
    assert_eq!(bound_array.rows, native_array.rows);

    let bounded_array = connection
        .query_params(
            "SELECT $1::value_contract_domain_array_label[] AS labels, \
             array_dims($1::value_contract_domain_array_label[])::text, \
             array_to_json($1::value_contract_domain_array_label[])::text, \
             pg_typeof($1::value_contract_domain_array_label[])::text, \
             array_lower($1::value_contract_domain_array_label[], 1), \
             array_upper($1::value_contract_domain_array_label[], 1), \
             array_lower($1::value_contract_domain_array_label[], 2), \
             array_upper($1::value_contract_domain_array_label[], 2)",
            &[Value::Text(r#"[0:1][3:4]={{"NULL",""},{"東京",NULL}}"#.into())],
        )
        .await
        .unwrap();
    assert_eq!(
        bounded_array.rows,
        vec![vec![
            Value::Text("[0:1][3:4]={{\"NULL\",\"\"},{\"東京\",NULL}}".into()),
            Value::Text("[0:1][3:4]".into()),
            Value::Text("[[\"NULL\",\"\"],[\"東京\",null]]".into()),
            Value::Text("value_contract_domain_array_label[]".into()),
            Value::Int(0),
            Value::Int(1),
            Value::Int(3),
            Value::Int(4),
        ]]
    );

    connection
        .execute(
            "CREATE TABLE value_contract_domain_array_import \
             (id integer PRIMARY KEY, labels value_contract_domain_array_label[])",
        )
        .await
        .unwrap();
    let source = connection
        .query(
            "SELECT 1 AS id, ARRAY[\
                 'NULL'::value_contract_domain_array_label, \
                 ''::value_contract_domain_array_label, \
                 '東京'::value_contract_domain_array_label, \
                 'a,b'::value_contract_domain_array_label, \
                 NULL::value_contract_domain_array_label\
             ] AS labels",
        )
        .await
        .unwrap();
    let csv_options = tablepro_core::export::CsvOptions::default();
    let csv = tablepro_core::export::render_csv(&source.columns, &source.rows, &csv_options);
    let import_options = tablepro_core::import::CsvImportOptions::default();
    let sheet = tablepro_core::import::read_csv(csv.as_bytes(), &import_options, None).unwrap();
    let columns = connection
        .fetch_columns(None, "value_contract_domain_array_import")
        .await
        .unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "public".into(),
            name: "value_contract_domain_array_label".into(),
        })
    );
    let mapping = [Some(0), Some(1)];
    let plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "postgres",

            capabilities: tablepro_core::DriverCapabilities::default(),
            schema: None,
            table: "value_contract_domain_array_import",
            columns: &columns,
            mapping: &mapping,
        },
        &sheet,
        &import_options,
    )
    .unwrap();
    assert!(
        plan.statement
            .contains("$2::text::\"public\".\"value_contract_domain_array_label\"[]")
    );
    connection.execute_params(&plan.statement, &plan.rows[0]).await.unwrap();
    let imported = connection
        .query(
            "SELECT pg_typeof(labels)::text, array_to_json(labels)::text, \
                    encode(array_send(labels), 'hex') \
             FROM value_contract_domain_array_import",
        )
        .await
        .unwrap();
    let native = connection
        .query(
            "SELECT pg_typeof(labels)::text, array_to_json(labels)::text, \
                    encode(array_send(labels), 'hex') \
             FROM (SELECT ARRAY[\
                 'NULL'::value_contract_domain_array_label, \
                 ''::value_contract_domain_array_label, \
                 '東京'::value_contract_domain_array_label, \
                 'a,b'::value_contract_domain_array_label, \
                 NULL::value_contract_domain_array_label\
             ] AS labels) AS source",
        )
        .await
        .unwrap();
    assert_eq!(imported.rows, native.rows);

    let edited = r#"{"NULL","",東京,NULL}"#;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "value_contract_domain_array_import",
        &columns,
        &[(1, Value::Text(edited.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(
        update
            .0
            .contains("$1::text::\"public\".\"value_contract_domain_array_label\"[]")
    );
    connection.execute_in_transaction(&[update]).await.unwrap();
    let grid_native = connection
        .query(
            "SELECT pg_typeof(labels)::text, array_to_json(labels)::text, \
                    encode(array_send(labels), 'hex') \
             FROM (SELECT ARRAY[\
                 'NULL'::value_contract_domain_array_label, \
                 ''::value_contract_domain_array_label, \
                 '東京'::value_contract_domain_array_label, \
                 NULL::value_contract_domain_array_label\
             ] AS labels) AS source",
        )
        .await
        .unwrap();
    let saved_grid = connection
        .query(
            "SELECT pg_typeof(labels)::text, array_to_json(labels)::text, \
                    encode(array_send(labels), 'hex') \
             FROM value_contract_domain_array_import",
        )
        .await
        .unwrap();
    assert_eq!(saved_grid.rows, grid_native.rows);

    let filters = FilterSet {
        rules: vec![FilterRule {
            column: "labels".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Single(edited.into())),
        }],
        ..Default::default()
    };
    let (predicate, params) = tablepro_core::filter::build_filter_where("postgres", &columns, &filters)
        .unwrap()
        .unwrap();
    assert!(predicate.contains("$1::\"public\".\"value_contract_domain_array_label\"[]"));
    let filtered = connection
        .query_params(
            &format!(
                "SELECT id, pg_typeof(labels)::text, array_to_json(labels)::text, \
                        encode(array_send(labels), 'hex') \
                 FROM value_contract_domain_array_import WHERE {predicate}"
            ),
            &params,
        )
        .await
        .unwrap();
    assert_eq!(
        filtered.rows,
        vec![vec![
            Value::Int(1),
            grid_native.rows[0][0].clone(),
            grid_native.rows[0][1].clone(),
            grid_native.rows[0][2].clone()
        ]]
    );

    let invalid_grid = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "value_contract_domain_array_import",
        &columns,
        &[(1, Value::Text("{forbidden}".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    let error = connection.execute_in_transaction(&[invalid_grid]).await.unwrap_err();
    let cause = match &error {
        tablepro_core::DriverError::Transaction { source, .. }
        | tablepro_core::DriverError::TransactionRollbackFailed { source, .. } => source.as_ref(),
        other => other,
    };
    assert!(
        matches!(
            cause,
            tablepro_core::DriverError::Query { sqlstate: Some(code), .. } if code == "23514"
        ),
        "unexpected invalid grid edit error: {error:?}"
    );
    assert_eq!(
        connection
            .query(
                "SELECT pg_typeof(labels)::text, array_to_json(labels)::text, \
                        encode(array_send(labels), 'hex') \
                 FROM value_contract_domain_array_import",
            )
            .await
            .unwrap()
            .rows,
        grid_native.rows
    );

    let invalid_domain = tablepro_core::sql_dialect::build_insert_from_draft(
        "postgres",
        None,
        "value_contract_domain_array_import",
        &columns,
        &[Value::Int(2), Value::Text("{forbidden}".into())],
    )
    .unwrap();
    let error = connection
        .execute_params(&invalid_domain.0, &invalid_domain.1)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        tablepro_core::DriverError::Query { sqlstate: Some(code), .. } if code == "23514"
    ));
    assert_eq!(
        connection
            .query("SELECT count(*)::bigint FROM value_contract_domain_array_import")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]]
    );

    let invalid_array = connection
        .query_params(
            "SELECT $1::value_contract_domain_array_label[]",
            &[Value::Text(r#"{"NULL","not-a-label"}"#.into())],
        )
        .await
        .unwrap_err();
    assert!(matches!(
        invalid_array,
        tablepro_core::DriverError::Query { sqlstate: Some(code), .. } if code == "22P02"
    ));
}
