use tablepro_core::{FilterOp, FilterRule, FilterSet, FilterValue, Value};

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_projection_preserves_labels_and_null() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE TYPE value_contract_domain_enum AS ENUM ('NULL', '東京')")
        .await
        .unwrap();
    connection
        .execute("CREATE DOMAIN value_contract_domain_enum_label AS value_contract_domain_enum")
        .await
        .unwrap();

    let result = connection
        .query(
            "SELECT label, pg_typeof(label)::text AS native_type FROM (VALUES \
             ('NULL'::value_contract_domain_enum_label), \
             ('東京'::value_contract_domain_enum_label), \
             (NULL::value_contract_domain_enum_label)) AS labels(label)",
        )
        .await
        .unwrap();

    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Text("NULL".into()),
                Value::Text("value_contract_domain_enum_label".into())
            ],
            vec![
                Value::Text("東京".into()),
                Value::Text("value_contract_domain_enum_label".into())
            ],
            vec![Value::Null, Value::Text("value_contract_domain_enum_label".into())],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_array_preserves_labels_and_null() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE TYPE value_contract_domain_array_enum AS ENUM ('NULL', '', '東京', 'a,b')")
        .await
        .unwrap();
    connection
        .execute("CREATE DOMAIN value_contract_domain_array_label AS value_contract_domain_array_enum")
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
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_parameters_preserve_native_type() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE TYPE value_contract_domain_param_enum AS ENUM ('NULL', '東京', 'ready')")
        .await
        .unwrap();
    connection
        .execute("CREATE DOMAIN value_contract_domain_param_label AS value_contract_domain_param_enum")
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE value_contract_domain_param_rows (id INT PRIMARY KEY, label value_contract_domain_param_label)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO value_contract_domain_param_rows VALUES (1, 'NULL'), (2, NULL), (3, '東京')")
        .await
        .unwrap();

    let matched = connection
        .query_params(
            "SELECT $1::value_contract_domain_param_label::text, \
             pg_typeof($1::value_contract_domain_param_label)::text",
            &[Value::Text("NULL".into())],
        )
        .await
        .unwrap();
    assert_eq!(
        matched.rows,
        vec![vec![
            Value::Text("NULL".into()),
            Value::Text("value_contract_domain_param_label".into()),
        ]]
    );

    let bound_null = connection
        .query_params(
            "SELECT $1::value_contract_domain_param_label, \
             pg_typeof($1::value_contract_domain_param_label)::text",
            &[Value::Null],
        )
        .await
        .unwrap();
    assert_eq!(
        bound_null.rows,
        vec![vec![
            Value::Null,
            Value::Text("value_contract_domain_param_label".into())
        ]]
    );

    let updated = connection
        .execute_params(
            "UPDATE value_contract_domain_param_rows SET label = $1::value_contract_domain_param_label WHERE id = 3",
            &[Value::Text("ready".into())],
        )
        .await
        .unwrap();
    assert_eq!(updated.rows_affected, 1);
    let stored = connection
        .query("SELECT label::text, pg_typeof(label)::text FROM value_contract_domain_param_rows WHERE id = 3")
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![vec![
            Value::Text("ready".into()),
            Value::Text("value_contract_domain_param_label".into()),
        ]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_filters_preserve_values_and_type() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_domain_filter")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_domain_filter.status AS ENUM ('NULL', 'ready', '東京')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_domain_filter.status_domain \
             AS value_contract_domain_filter.status",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_domain_filter.rows \
             (id INT PRIMARY KEY, label value_contract_domain_filter.status_domain)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_domain_filter.rows VALUES \
             (1, 'NULL'), (2, 'ready'), (3, '東京'), (4, NULL)",
        )
        .await
        .unwrap();

    let columns = connection
        .fetch_columns(Some("value_contract_domain_filter"), "rows")
        .await
        .unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "value_contract_domain_filter".into(),
            name: "status".into(),
        })
    );
    let cases = [
        (FilterOp::Eq, FilterValue::Single("NULL".into()), vec![(1, "NULL")]),
        (
            FilterOp::In,
            FilterValue::List(vec!["ready".into(), "東京".into()]),
            vec![(2, "ready"), (3, "東京")],
        ),
    ];
    for (op, value, expected_ids) in cases {
        let filters = FilterSet {
            rules: vec![FilterRule {
                column: "label".into(),
                op,
                value: Some(value),
            }],
            ..Default::default()
        };
        let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &filters)
            .unwrap()
            .unwrap();
        let result = connection
            .query_params(
                &format!(
                    "SELECT id, label::text, pg_typeof(label)::text \
                     FROM value_contract_domain_filter.rows WHERE {where_sql} ORDER BY id"
                ),
                &params,
            )
            .await
            .unwrap();
        let expected = expected_ids
            .into_iter()
            .map(|(id, label)| {
                vec![
                    Value::Int(id),
                    Value::Text(label.into()),
                    Value::Text("value_contract_domain_filter.status_domain".into()),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(result.rows, expected);
    }

    let null_filter = FilterSet {
        rules: vec![FilterRule {
            column: "label".into(),
            op: FilterOp::IsNull,
            value: None,
        }],
        ..Default::default()
    };
    let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &null_filter)
        .unwrap()
        .unwrap();
    let result = connection
        .query_params(
            &format!(
                "SELECT id, label::text, pg_typeof(label)::text \
                 FROM value_contract_domain_filter.rows WHERE {where_sql}"
            ),
            &params,
        )
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![vec![
            Value::Int(4),
            Value::Null,
            Value::Text("value_contract_domain_filter.status_domain".into())
        ]]
    );

    let (update_sql, update_params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some("value_contract_domain_filter"),
        "rows",
        &columns,
        &[(1, Value::Text("東京".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    connection.execute_params(&update_sql, &update_params).await.unwrap();
    let (insert_sql, insert_params) = tablepro_core::sql_dialect::build_insert_from_draft(
        "postgres",
        Some("value_contract_domain_filter"),
        "rows",
        &columns,
        &[Value::Int(5), Value::Text("ready".into())],
    )
    .unwrap();
    connection.execute_params(&insert_sql, &insert_params).await.unwrap();
    let writes = connection
        .query(
            "SELECT id, label::text, pg_typeof(label)::text \
             FROM value_contract_domain_filter.rows WHERE id IN (1, 5) ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        writes.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("東京".into()),
                Value::Text("value_contract_domain_filter.status_domain".into()),
            ],
            vec![
                Value::Int(5),
                Value::Text("ready".into()),
                Value::Text("value_contract_domain_filter.status_domain".into()),
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_csv_round_trip_preserves_values_and_type() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    for sql in [
        "CREATE SCHEMA value_contract_domain_csv",
        "CREATE TYPE value_contract_domain_csv.status AS ENUM ('NULL', '', 'ready', '東京')",
        "CREATE DOMAIN value_contract_domain_csv.status_domain AS value_contract_domain_csv.status",
        "CREATE TABLE value_contract_domain_csv.source_rows (id INT PRIMARY KEY, label value_contract_domain_csv.status_domain)",
        "CREATE TABLE value_contract_domain_csv.target_rows (id INT PRIMARY KEY, label value_contract_domain_csv.status_domain)",
        "INSERT INTO value_contract_domain_csv.source_rows VALUES (1, 'NULL'), (2, ''), (3, 'ready'), (4, '東京'), (5, NULL)",
    ] {
        connection.execute(sql).await.unwrap();
    }

    let source = connection
        .query("SELECT id, label FROM value_contract_domain_csv.source_rows ORDER BY id")
        .await
        .unwrap();
    let default_csv = tablepro_core::export::render_csv(
        &source.columns,
        &source.rows,
        &tablepro_core::export::CsvOptions::default(),
    );
    let default_options = tablepro_core::import::CsvImportOptions::default();
    let default_sheet = tablepro_core::import::read_csv(default_csv.as_bytes(), &default_options, None).unwrap();
    let target_columns = connection
        .fetch_columns(Some("value_contract_domain_csv"), "target_rows")
        .await
        .unwrap();
    assert_eq!(
        target_columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "value_contract_domain_csv".into(),
            name: "status".into(),
        })
    );
    let target = tablepro_core::import::ImportTarget {
        driver_id: "postgres",
        schema: Some("value_contract_domain_csv"),
        table: "target_rows",
        columns: &target_columns,
        mapping: &[Some(0), Some(1)],
    };
    assert!(matches!(
        tablepro_core::import::build_insert_plan(&target, &default_sheet, &default_options),
        Err(tablepro_core::import::PlanError::Rows { total: 2, .. })
    ));
    let untouched = connection
        .query("SELECT count(*)::bigint FROM value_contract_domain_csv.target_rows")
        .await
        .unwrap();
    assert_eq!(untouched.rows, vec![vec![Value::Int(0)]]);

    let options = tablepro_core::import::CsvImportOptions {
        null_marker: "\\N".into(),
        ..Default::default()
    };
    let export_options = tablepro_core::export::CsvOptions {
        null_to_empty: false,
        null_marker: Some("\\N".into()),
        ..Default::default()
    };
    let csv = tablepro_core::export::render_csv(&source.columns, &source.rows, &export_options);
    assert_eq!(csv, "id,label\n1,NULL\n2,\"\"\n3,ready\n4,東京\n5,\\N\n");
    let sheet = tablepro_core::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    let plan = tablepro_core::import::build_insert_plan(&target, &sheet, &options).unwrap();
    assert!(
        plan.statement
            .contains("$2::text::\"value_contract_domain_csv\".\"status\"")
    );
    for row in &plan.rows {
        connection.execute_params(&plan.statement, row).await.unwrap();
    }

    let restored = connection
        .query(
            "SELECT id, label::text, pg_typeof(label)::text \
             FROM value_contract_domain_csv.target_rows ORDER BY id",
        )
        .await
        .unwrap();
    let domain_type = Value::Text("value_contract_domain_csv.status_domain".into());
    assert_eq!(
        restored.rows,
        vec![
            vec![Value::Int(1), Value::Text("NULL".into()), domain_type.clone()],
            vec![Value::Int(2), Value::Text(String::new()), domain_type.clone()],
            vec![Value::Int(3), Value::Text("ready".into()), domain_type.clone()],
            vec![Value::Int(4), Value::Text("東京".into()), domain_type],
            vec![
                Value::Int(5),
                Value::Null,
                Value::Text("value_contract_domain_csv.status_domain".into())
            ],
        ]
    );
}
