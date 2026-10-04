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

    let expected_json = serde_json::json!([
        {"label": "NULL", "native_type": "value_contract_domain_enum_label"},
        {"label": "東京", "native_type": "value_contract_domain_enum_label"},
        {"label": null, "native_type": "value_contract_domain_enum_label"}
    ]);
    let rendered = tablepro_core::export::render_json(&result.columns, &result.rows);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&rendered).unwrap(),
        expected_json
    );

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("domain-enum.json");
    tablepro_core::export::write_result_file(
        &path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Json,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let file_json: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file_json, expected_json);
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
async fn value_contract_domain_over_enum_assignment_infers_parameter_type() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_domain_infer")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_domain_infer.state AS ENUM ('NULL', '東京', 'ready')")
        .await
        .unwrap();
    connection
        .execute("CREATE DOMAIN value_contract_domain_infer.state_domain AS value_contract_domain_infer.state")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_domain_infer.rows \
             (id INT PRIMARY KEY, status value_contract_domain_infer.state_domain)",
        )
        .await
        .unwrap();
    connection
        .execute("INSERT INTO value_contract_domain_infer.rows VALUES (1, 'NULL'), (2, '東京')")
        .await
        .unwrap();

    let updated = connection
        .execute_params(
            "UPDATE value_contract_domain_infer.rows SET status = $1 WHERE id = 2",
            &[Value::Text("ready".into())],
        )
        .await
        .unwrap();
    assert_eq!(updated.rows_affected, 1);
    let stored = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_domain_infer.rows WHERE id = 2",
        )
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![vec![
            Value::Int(2),
            Value::Text("ready".into()),
            Value::Text("value_contract_domain_infer.state_domain".into()),
        ]]
    );

    let nulled = connection
        .execute_params(
            "UPDATE value_contract_domain_infer.rows SET status = $1 WHERE id = 2",
            &[Value::Null],
        )
        .await
        .unwrap();
    assert_eq!(nulled.rows_affected, 1);
    let stored_null = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_domain_infer.rows WHERE id = 2",
        )
        .await
        .unwrap();
    assert_eq!(
        stored_null.rows,
        vec![vec![
            Value::Int(2),
            Value::Null,
            Value::Text("value_contract_domain_infer.state_domain".into()),
        ]]
    );
    let sibling = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_domain_infer.rows WHERE id = 1",
        )
        .await
        .unwrap();
    assert_eq!(
        sibling.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("NULL".into()),
            Value::Text("value_contract_domain_infer.state_domain".into()),
        ]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_query_comparison_infers_parameter_type() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_domain_query")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_domain_query.state AS ENUM ('ready', 'paused')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_domain_query.state_domain \
             AS value_contract_domain_query.state",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_domain_query.rows \
             (id INT PRIMARY KEY, status value_contract_domain_query.state_domain)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_domain_query.rows VALUES \
             (1, 'ready'), (2, 'paused'), (3, NULL)",
        )
        .await
        .unwrap();

    let uncast = connection
        .query_params(
            "SELECT id, status = $1, pg_typeof($1)::text, pg_typeof(status)::text \
             FROM value_contract_domain_query.rows ORDER BY id",
            &[Value::Text("ready".into())],
        )
        .await
        .expect_err("raw domain equality has no domain-to-unknown operator");
    assert!(matches!(
        uncast,
        tablepro_core::DriverError::Query {
            sqlstate: Some(code), ..
        } if code == "42883"
    ));

    let result = connection
        .query_params(
            "SELECT id, status::value_contract_domain_query.state = $1, \
             pg_typeof($1)::text, pg_typeof(status)::text \
             FROM value_contract_domain_query.rows ORDER BY id",
            &[Value::Text("ready".into())],
        )
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Bool(true),
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state_domain".into()),
            ],
            vec![
                Value::Int(2),
                Value::Bool(false),
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state_domain".into()),
            ],
            vec![
                Value::Int(3),
                Value::Null,
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state_domain".into()),
            ],
        ]
    );

    let not_equal = connection
        .query_params(
            "SELECT id, status::value_contract_domain_query.state <> $1, \
             pg_typeof($1)::text, pg_typeof(status)::text \
             FROM value_contract_domain_query.rows ORDER BY id",
            &[Value::Text("paused".into())],
        )
        .await
        .unwrap();
    assert_eq!(
        not_equal.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Bool(true),
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state_domain".into()),
            ],
            vec![
                Value::Int(2),
                Value::Bool(false),
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state_domain".into()),
            ],
            vec![
                Value::Int(3),
                Value::Null,
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state_domain".into()),
            ],
        ]
    );

    for (operator, parameter, expected) in [
        (
            "IS DISTINCT FROM",
            Value::Text("ready".into()),
            [Some(false), Some(true), Some(true)],
        ),
        (
            "IS NOT DISTINCT FROM",
            Value::Text("ready".into()),
            [Some(true), Some(false), Some(false)],
        ),
        ("IS DISTINCT FROM", Value::Null, [Some(true), Some(true), Some(false)]),
        (
            "IS NOT DISTINCT FROM",
            Value::Null,
            [Some(false), Some(false), Some(true)],
        ),
    ] {
        let result = connection
            .query_params(
                &format!(
                    "SELECT id, status::value_contract_domain_query.state {operator} $1, \
                     pg_typeof($1)::text, pg_typeof(status)::text \
                     FROM value_contract_domain_query.rows ORDER BY id"
                ),
                std::slice::from_ref(&parameter),
            )
            .await
            .unwrap();
        let expected = expected
            .into_iter()
            .enumerate()
            .map(|(index, comparison)| {
                vec![
                    Value::Int(index as i64 + 1),
                    Value::Bool(comparison.unwrap()),
                    Value::Text("value_contract_domain_query.state".into()),
                    Value::Text("value_contract_domain_query.state_domain".into()),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(result.rows, expected, "operator {operator}, parameter {parameter:?}");
    }

    for (operator, parameter, expected) in [
        ("<", "paused", [Some(true), Some(false), None]),
        ("<=", "paused", [Some(true), Some(true), None]),
        (">", "ready", [Some(false), Some(true), None]),
        (">=", "ready", [Some(true), Some(true), None]),
    ] {
        let result = connection
            .query_params(
                &format!(
                    "SELECT id, status::value_contract_domain_query.state {operator} $1, \
                     pg_typeof($1)::text, pg_typeof(status)::text \
                     FROM value_contract_domain_query.rows ORDER BY id"
                ),
                &[Value::Text(parameter.into())],
            )
            .await
            .unwrap();
        let expected = expected
            .into_iter()
            .enumerate()
            .map(|(index, comparison)| {
                vec![
                    Value::Int(index as i64 + 1),
                    comparison.map_or(Value::Null, Value::Bool),
                    Value::Text("value_contract_domain_query.state".into()),
                    Value::Text("value_contract_domain_query.state_domain".into()),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(result.rows, expected, "operator {operator}");
    }

    for (operator, expected) in [
        ("IN", [Some(true), Some(true), None]),
        ("NOT IN", [Some(false), Some(false), None]),
    ] {
        let result = connection
            .query_params(
                &format!(
                    "SELECT id, status::value_contract_domain_query.state {operator} ($1, $2), \
                     pg_typeof($1)::text, pg_typeof($2)::text, pg_typeof(status)::text \
                     FROM value_contract_domain_query.rows ORDER BY id"
                ),
                &[Value::Text("ready".into()), Value::Text("paused".into())],
            )
            .await
            .unwrap();
        let expected = expected
            .into_iter()
            .enumerate()
            .map(|(index, comparison)| {
                vec![
                    Value::Int(index as i64 + 1),
                    comparison.map_or(Value::Null, Value::Bool),
                    Value::Text("value_contract_domain_query.state".into()),
                    Value::Text("value_contract_domain_query.state".into()),
                    Value::Text("value_contract_domain_query.state_domain".into()),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(result.rows, expected, "operator {operator}");
    }

    let between = connection
        .query_params(
            "SELECT id, status::value_contract_domain_query.state BETWEEN $1 AND $2, \
             pg_typeof($1)::text, pg_typeof($2)::text, pg_typeof(status)::text \
             FROM value_contract_domain_query.rows ORDER BY id",
            &[Value::Text("ready".into()), Value::Text("paused".into())],
        )
        .await
        .unwrap();
    assert_eq!(
        between.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Bool(true),
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state_domain".into()),
            ],
            vec![
                Value::Int(2),
                Value::Bool(true),
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state_domain".into()),
            ],
            vec![
                Value::Int(3),
                Value::Null,
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state".into()),
                Value::Text("value_contract_domain_query.state_domain".into()),
            ],
        ]
    );

    let null_result = connection
        .query_params(
            "SELECT status::value_contract_domain_query.state = $1, \
             pg_typeof($1)::text, pg_typeof(status)::text \
             FROM value_contract_domain_query.rows WHERE id = 1",
            &[Value::Null],
        )
        .await
        .unwrap();
    assert_eq!(
        null_result.rows,
        vec![vec![
            Value::Null,
            Value::Text("value_contract_domain_query.state".into()),
            Value::Text("value_contract_domain_query.state_domain".into()),
        ]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_three_level_domain_over_enum_infers_parameters_and_decodes() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_nested_domain")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_nested_domain.state AS ENUM ('NULL', '', '東京', 'ready')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_nested_domain.state_inner \
             AS value_contract_nested_domain.state",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_nested_domain.state_middle \
             AS value_contract_nested_domain.state_inner",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_nested_domain.state_outer \
             AS value_contract_nested_domain.state_middle",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_nested_domain.rows \
             (id INT PRIMARY KEY, status value_contract_nested_domain.state_outer)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_nested_domain.rows VALUES \
             (1, 'NULL'), (2, '東京'), (3, '東京')",
        )
        .await
        .unwrap();

    let columns = connection
        .fetch_columns(Some("value_contract_nested_domain"), "rows")
        .await
        .unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "value_contract_nested_domain".into(),
            name: "state".into(),
        })
    );

    let projected = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_nested_domain.rows ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        projected.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("NULL".into()),
                Value::Text("value_contract_nested_domain.state_outer".into()),
            ],
            vec![
                Value::Int(2),
                Value::Text("東京".into()),
                Value::Text("value_contract_nested_domain.state_outer".into()),
            ],
            vec![
                Value::Int(3),
                Value::Text("東京".into()),
                Value::Text("value_contract_nested_domain.state_outer".into()),
            ],
        ]
    );

    let uncast_comparison = connection
        .query_params(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_nested_domain.rows WHERE status = $1",
            &[Value::Text("NULL".into())],
        )
        .await
        .expect_err("PostgreSQL cannot resolve domain = unknown without an explicit cast");
    assert!(matches!(
        uncast_comparison,
        tablepro_core::DriverError::Query {
            sqlstate: Some(code), ..
        } if code == "42883"
    ));
    let typed_comparison = connection
        .query_params(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_nested_domain.rows \
             WHERE status::value_contract_nested_domain.state = \
                   $1::value_contract_nested_domain.state",
            &[Value::Text("NULL".into())],
        )
        .await
        .unwrap();
    assert_eq!(
        typed_comparison.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("NULL".into()),
            Value::Text("value_contract_nested_domain.state_outer".into()),
        ]]
    );

    let updated = connection
        .execute_params(
            "UPDATE value_contract_nested_domain.rows SET status = $1 WHERE id = 2",
            &[Value::Text("ready".into())],
        )
        .await
        .unwrap();
    assert_eq!(updated.rows_affected, 1);
    let invalid = connection
        .execute_params(
            "UPDATE value_contract_nested_domain.rows SET status = $1 WHERE id = 2",
            &[Value::Text("not a label".into())],
        )
        .await
        .expect_err("invalid nested-domain enum labels must be refused by PostgreSQL");
    assert!(matches!(
        invalid,
        tablepro_core::DriverError::Query {
            sqlstate: Some(code), ..
        } if code == "22P02"
    ));
    let after_invalid = connection
        .query(
            "SELECT status::text, pg_typeof(status)::text \
             FROM value_contract_nested_domain.rows WHERE id = 2",
        )
        .await
        .unwrap();
    assert_eq!(
        after_invalid.rows,
        vec![vec![
            Value::Text("ready".into()),
            Value::Text("value_contract_nested_domain.state_outer".into()),
        ]]
    );

    let literal_null_filter = FilterSet {
        rules: vec![FilterRule {
            column: "status".into(),
            op: FilterOp::Eq,
            value: Some(tablepro_core::FilterValue::Single("NULL".into())),
        }],
        ..Default::default()
    };
    let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &literal_null_filter)
        .unwrap()
        .unwrap();
    let filtered = connection
        .query_params(
            &format!(
                "SELECT id, status::text, pg_typeof(status)::text \
                 FROM value_contract_nested_domain.rows WHERE {where_sql}"
            ),
            &params,
        )
        .await
        .unwrap();
    assert_eq!(
        filtered.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("NULL".into()),
            Value::Text("value_contract_nested_domain.state_outer".into()),
        ]]
    );

    let (update_sql, update_params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some("value_contract_nested_domain"),
        "rows",
        &columns,
        &[(1, Value::Text("東京".into()))],
        &[Value::Int(2)],
    )
    .unwrap();
    connection.execute_params(&update_sql, &update_params).await.unwrap();
    let (insert_sql, insert_params) = tablepro_core::sql_dialect::build_insert_from_draft(
        "postgres",
        Some("value_contract_nested_domain"),
        "rows",
        &columns,
        &[Value::Int(4), Value::Text("NULL".into())],
    )
    .unwrap();
    connection.execute_params(&insert_sql, &insert_params).await.unwrap();

    let transaction = connection
        .execute_in_transaction(&[
            (
                "UPDATE value_contract_nested_domain.rows SET status = $1 WHERE id = 1".into(),
                vec![Value::Text(String::new())],
            ),
            (
                "UPDATE value_contract_nested_domain.rows SET status = $1 WHERE id = 3".into(),
                vec![Value::Null],
            ),
        ])
        .await
        .unwrap();
    assert_eq!(transaction, vec![1, 1]);

    for (operator, parameter, expected) in [
        (
            "IS DISTINCT FROM",
            Value::Text("東京".into()),
            [true, false, true, true],
        ),
        (
            "IS NOT DISTINCT FROM",
            Value::Text("東京".into()),
            [false, true, false, false],
        ),
        ("IS DISTINCT FROM", Value::Null, [true, true, false, true]),
        ("IS NOT DISTINCT FROM", Value::Null, [false, false, true, false]),
    ] {
        let result = connection
            .query_params(
                &format!(
                    "SELECT id, status::value_contract_nested_domain.state {operator} $1, \
                     pg_typeof($1)::text, pg_typeof(status)::text \
                     FROM value_contract_nested_domain.rows ORDER BY id"
                ),
                std::slice::from_ref(&parameter),
            )
            .await
            .unwrap();
        let expected = expected
            .into_iter()
            .enumerate()
            .map(|(index, comparison)| {
                vec![
                    Value::Int(index as i64 + 1),
                    Value::Bool(comparison),
                    Value::Text("value_contract_nested_domain.state".into()),
                    Value::Text("value_contract_nested_domain.state_outer".into()),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(result.rows, expected, "operator {operator}, parameter {parameter:?}");
    }

    let typed_null = connection
        .query_params(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_nested_domain.rows \
             WHERE status::value_contract_nested_domain.state \
                   IS NOT DISTINCT FROM $1::value_contract_nested_domain.state",
            &[Value::Null],
        )
        .await
        .unwrap();
    assert_eq!(
        typed_null.rows,
        vec![vec![
            Value::Int(3),
            Value::Null,
            Value::Text("value_contract_nested_domain.state_outer".into()),
        ]]
    );
    let stored = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_nested_domain.rows ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text(String::new()),
                Value::Text("value_contract_nested_domain.state_outer".into()),
            ],
            vec![
                Value::Int(2),
                Value::Text("東京".into()),
                Value::Text("value_contract_nested_domain.state_outer".into()),
            ],
            vec![
                Value::Int(3),
                Value::Null,
                Value::Text("value_contract_nested_domain.state_outer".into()),
            ],
            vec![
                Value::Int(4),
                Value::Text("NULL".into()),
                Value::Text("value_contract_nested_domain.state_outer".into()),
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_four_domain_levels_over_enum_preserve_metadata_and_keyed_values() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_four_domains")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_four_domains.state AS ENUM ('ready', 'paused')")
        .await
        .unwrap();

    let mut base_type = "state".to_owned();
    for level in 1..=4 {
        let domain = format!("state_domain_{level}");
        connection
            .execute(&format!(
                "CREATE DOMAIN value_contract_four_domains.{domain} \
                 AS value_contract_four_domains.{base_type}"
            ))
            .await
            .unwrap();
        base_type = domain;
    }
    connection
        .execute(&format!(
            "CREATE TABLE value_contract_four_domains.rows \
             (id INT PRIMARY KEY, status value_contract_four_domains.{base_type})"
        ))
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_four_domains.rows VALUES \
             (1, 'ready'), (2, NULL), (3, 'ready')",
        )
        .await
        .unwrap();

    let columns = connection
        .fetch_columns(Some("value_contract_four_domains"), "rows")
        .await
        .unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "value_contract_four_domains".into(),
            name: "state".into(),
        })
    );

    let (update_sql, update_params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some("value_contract_four_domains"),
        "rows",
        &columns,
        &[(1, Value::Text("paused".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    connection.execute_params(&update_sql, &update_params).await.unwrap();

    let inferred_query = connection
        .query_params(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_four_domains.rows \
             WHERE status::value_contract_four_domains.state = $1 ORDER BY id",
            &[Value::Text("paused".into())],
        )
        .await
        .unwrap();
    assert_eq!(
        inferred_query.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("paused".into()),
            Value::Text("value_contract_four_domains.state_domain_4".into()),
        ]]
    );

    let ambiguous_parameter = connection
        .execute(
            "PREPARE four_domain_ambiguous_parameter AS \
             SELECT pg_typeof($1)::text FROM value_contract_four_domains.rows \
             WHERE status::value_contract_four_domains.state = $1",
        )
        .await
        .expect_err("pg_typeof leaves this enum comparison parameter ambiguous");
    assert!(matches!(
        ambiguous_parameter,
        tablepro_core::DriverError::Query {
            sqlstate: Some(code), ..
        } if code == "42P08"
    ));

    let explicitly_typed = connection
        .query_params(
            "SELECT id, pg_typeof($1::value_contract_four_domains.state)::text, \
             pg_typeof(status)::text \
             FROM value_contract_four_domains.rows \
             WHERE status::value_contract_four_domains.state = \
                   $1::value_contract_four_domains.state ORDER BY id",
            &[Value::Text("paused".into())],
        )
        .await
        .unwrap();
    assert_eq!(
        explicitly_typed.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("value_contract_four_domains.state".into()),
            Value::Text("value_contract_four_domains.state_domain_4".into()),
        ]]
    );

    let inferred_update = connection
        .execute_params(
            "UPDATE value_contract_four_domains.rows SET status = $1 WHERE id = 3",
            &[Value::Text("paused".into())],
        )
        .await
        .unwrap();
    assert_eq!(inferred_update.rows_affected, 1);

    let filters = FilterSet {
        rules: vec![FilterRule {
            column: "status".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Single("paused".into())),
        }],
        ..Default::default()
    };
    let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &filters)
        .unwrap()
        .unwrap();
    let filtered = connection
        .query_params(
            &format!(
                "SELECT id, status::text, pg_typeof(status)::text \
                 FROM value_contract_four_domains.rows WHERE {where_sql} ORDER BY id"
            ),
            &params,
        )
        .await
        .unwrap();
    assert_eq!(
        filtered.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("paused".into()),
                Value::Text("value_contract_four_domains.state_domain_4".into()),
            ],
            vec![
                Value::Int(3),
                Value::Text("paused".into()),
                Value::Text("value_contract_four_domains.state_domain_4".into()),
            ],
        ]
    );

    let stored = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_four_domains.rows ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("paused".into()),
                Value::Text("value_contract_four_domains.state_domain_4".into()),
            ],
            vec![
                Value::Int(2),
                Value::Null,
                Value::Text("value_contract_four_domains.state_domain_4".into()),
            ],
            vec![
                Value::Int(3),
                Value::Text("paused".into()),
                Value::Text("value_contract_four_domains.state_domain_4".into()),
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_five_domain_levels_over_enum_preserve_metadata_and_values() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_five_domains")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_five_domains.state AS ENUM ('ready', 'paused')")
        .await
        .unwrap();

    let mut base_type = "state".to_owned();
    for level in 1..=5 {
        let domain = format!("state_domain_{level}");
        connection
            .execute(&format!(
                "CREATE DOMAIN value_contract_five_domains.{domain} \
                 AS value_contract_five_domains.{base_type}"
            ))
            .await
            .unwrap();
        base_type = domain;
    }
    connection
        .execute(&format!(
            "CREATE TABLE value_contract_five_domains.rows \
             (id INT PRIMARY KEY, status value_contract_five_domains.{base_type})"
        ))
        .await
        .unwrap();
    connection
        .execute("INSERT INTO value_contract_five_domains.rows VALUES (1, 'ready'), (2, NULL), (3, 'ready')")
        .await
        .unwrap();

    let columns = connection
        .fetch_columns(Some("value_contract_five_domains"), "rows")
        .await
        .unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "value_contract_five_domains".into(),
            name: "state".into(),
        })
    );

    let (update_sql, update_params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some("value_contract_five_domains"),
        "rows",
        &columns,
        &[(1, Value::Text("paused".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    connection.execute_params(&update_sql, &update_params).await.unwrap();

    let inferred = connection
        .query_params(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_five_domains.rows \
             WHERE status::value_contract_five_domains.state = $1 ORDER BY id",
            &[Value::Text("paused".into())],
        )
        .await
        .unwrap();
    assert_eq!(
        inferred.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("paused".into()),
            Value::Text("value_contract_five_domains.state_domain_5".into()),
        ]]
    );

    let inferred_update = connection
        .execute_params(
            "UPDATE value_contract_five_domains.rows SET status = $1 WHERE id = 3",
            &[Value::Text("paused".into())],
        )
        .await
        .unwrap();
    assert_eq!(inferred_update.rows_affected, 1);
    let invalid = connection
        .execute_params(
            "UPDATE value_contract_five_domains.rows SET status = $1 WHERE id = 3",
            &[Value::Text("not a label".into())],
        )
        .await
        .expect_err("invalid enum label must be refused through five domain layers");
    assert!(matches!(
        invalid,
        tablepro_core::DriverError::Query {
            sqlstate: Some(code), ..
        } if code == "22P02"
    ));

    let filtered = FilterSet {
        rules: vec![FilterRule {
            column: "status".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Single("paused".into())),
        }],
        ..Default::default()
    };
    let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &filtered)
        .unwrap()
        .unwrap();
    let result = connection
        .query_params(
            &format!(
                "SELECT id, status::text, pg_typeof(status)::text \
                 FROM value_contract_five_domains.rows WHERE {where_sql} ORDER BY id"
            ),
            &params,
        )
        .await
        .unwrap();
    let outer_domain = Value::Text("value_contract_five_domains.state_domain_5".into());
    assert_eq!(
        result.rows,
        vec![
            vec![Value::Int(1), Value::Text("paused".into()), outer_domain.clone()],
            vec![Value::Int(3), Value::Text("paused".into()), outer_domain.clone()],
        ]
    );

    let stored = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM value_contract_five_domains.rows ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![
            vec![Value::Int(1), Value::Text("paused".into()), outer_domain.clone()],
            vec![Value::Int(2), Value::Null, outer_domain.clone()],
            vec![Value::Int(3), Value::Text("paused".into()), outer_domain],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_deep_domain_levels_over_enum_ignore_shadowed_search_path() {
    let (_container, opts) = start_pg().await;
    assert_domain_level_contract(opts.clone(), 6).await;
    assert_domain_level_contract(opts.clone(), 7).await;
    assert_domain_level_contract(opts.clone(), 8).await;
    assert_domain_level_contract(opts.clone(), 9).await;
    assert_domain_level_contract(opts.clone(), 10).await;
    assert_domain_level_contract(opts.clone(), 63).await;
    assert_domain_level_contract(opts.clone(), 64).await;
    assert_domain_level_contract(opts.clone(), 65).await;
    assert_domain_level_contract(opts, 128).await;
}

async fn assert_domain_level_contract(opts: tablepro_core::ConnectOptions, levels: usize) {
    let schema = format!("value_contract_{levels}_domains");
    let shadow_schema = format!("value_contract_{levels}_shadow");
    let setup = connect(opts.clone()).await;
    setup.execute(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    setup
        .execute(&format!("CREATE TYPE {schema}.state AS ENUM ('ready', 'paused')"))
        .await
        .unwrap();

    let mut base_type = "state".to_owned();
    for level in 1..=levels {
        let domain = format!("state_domain_{level}");
        setup
            .execute(&format!(
                "CREATE DOMAIN {schema}.{domain} \
                 AS {schema}.{base_type}"
            ))
            .await
            .unwrap();
        base_type = domain;
    }
    setup
        .execute(&format!(
            "CREATE TABLE {schema}.rows \
             (id INT PRIMARY KEY, status {schema}.{base_type})"
        ))
        .await
        .unwrap();
    setup
        .execute(&format!(
            "INSERT INTO {schema}.rows VALUES (1, 'ready'), (2, NULL), (3, 'ready')"
        ))
        .await
        .unwrap();

    setup.execute(&format!("CREATE SCHEMA {shadow_schema}")).await.unwrap();
    setup
        .execute(&format!("CREATE TYPE {shadow_schema}.state AS ENUM ('ready')"))
        .await
        .unwrap();
    setup
        .execute(&format!("ALTER ROLE postgres SET search_path TO {shadow_schema}"))
        .await
        .unwrap();
    drop(setup);

    let connection = connect(opts).await;
    assert_eq!(
        connection.query("SELECT current_schema()::text").await.unwrap().rows,
        vec![vec![Value::Text(shadow_schema.clone())]]
    );

    let columns = connection.fetch_columns(Some(&schema), "rows").await.unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: schema.clone(),
            name: "state".into(),
        })
    );
    let (update_sql, update_params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some(&schema),
        "rows",
        &columns,
        &[(1, Value::Text("paused".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    connection.execute_params(&update_sql, &update_params).await.unwrap();

    let (insert_sql, insert_params) = tablepro_core::sql_dialect::build_insert_from_draft(
        "postgres",
        Some(&schema),
        "rows",
        &columns,
        &[Value::Int(4), Value::Text("paused".into())],
    )
    .unwrap();
    connection.execute_params(&insert_sql, &insert_params).await.unwrap();
    let (insert_null_sql, insert_null_params) = tablepro_core::sql_dialect::build_insert_from_draft(
        "postgres",
        Some(&schema),
        "rows",
        &columns,
        &[Value::Int(5), Value::Null],
    )
    .unwrap();
    connection
        .execute_params(&insert_null_sql, &insert_null_params)
        .await
        .unwrap();

    let invalid = connection
        .execute_params(
            &format!("UPDATE {schema}.rows SET status = $1 WHERE id = 3"),
            &[Value::Text("not a label".into())],
        )
        .await
        .expect_err(&format!(
            "invalid enum label must be refused through {levels} domain layers"
        ));
    if levels >= 64 {
        assert!(
            matches!(&invalid, tablepro_core::DriverError::Unsupported(message)
                if message.contains("resolvable depth")),
            "depth {levels}: {invalid:?}"
        );
        let null_inference = connection
            .execute_params(
                &format!("UPDATE {schema}.rows SET status = $1 WHERE id = 3"),
                &[Value::Null],
            )
            .await
            .expect_err(&format!("raw inferred SQL NULL must be refused at depth {levels}"));
        assert!(
            matches!(&null_inference, tablepro_core::DriverError::Unsupported(message)
                if message.contains("resolvable depth")),
            "depth {levels}: {null_inference:?}"
        );
    } else {
        assert!(
            matches!(
                &invalid,
                tablepro_core::DriverError::Query {
                    sqlstate: Some(code), ..
                } if code == "22P02"
            ),
            "depth {levels}: {invalid:?}"
        );

        let null_update = connection
            .execute_params(
                &format!("UPDATE {schema}.rows SET status = $1 WHERE id = 3"),
                &[Value::Null],
            )
            .await
            .unwrap();
        assert_eq!(null_update.rows_affected, 1);
        let null_row = connection
            .query(&format!(
                "SELECT status IS NULL, pg_typeof(status)::text \
                 FROM {schema}.rows WHERE id = 3"
            ))
            .await
            .unwrap();
        assert_eq!(
            null_row.rows,
            vec![vec![
                Value::Bool(true),
                Value::Text(format!("{schema}.state_domain_{levels}")),
            ]]
        );
        let restore = connection
            .execute_params(
                &format!("UPDATE {schema}.rows SET status = $1 WHERE id = 3"),
                &[Value::Text("ready".into())],
            )
            .await
            .unwrap();
        assert_eq!(restore.rows_affected, 1);
    }

    let filtered = FilterSet {
        rules: vec![FilterRule {
            column: "status".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Single("paused".into())),
        }],
        ..Default::default()
    };
    let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &filtered)
        .unwrap()
        .unwrap();
    let result = connection
        .query_params(
            &format!(
                "SELECT id, status::text, pg_typeof(status)::text \
                 FROM {schema}.rows WHERE {where_sql} ORDER BY id"
            ),
            &params,
        )
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("paused".into()),
                Value::Text(format!("{schema}.state_domain_{levels}")),
            ],
            vec![
                Value::Int(4),
                Value::Text("paused".into()),
                Value::Text(format!("{schema}.state_domain_{levels}")),
            ],
        ]
    );

    let stored = connection
        .query(&format!(
            "SELECT id, status::text, pg_typeof(status)::text FROM {schema}.rows ORDER BY id"
        ))
        .await
        .unwrap();
    let outer_domain = Value::Text(format!("{schema}.state_domain_{levels}"));
    assert_eq!(
        stored.rows,
        vec![
            vec![Value::Int(1), Value::Text("paused".into()), outer_domain.clone()],
            vec![Value::Int(2), Value::Null, outer_domain.clone()],
            vec![Value::Int(3), Value::Text("ready".into()), outer_domain],
            vec![
                Value::Int(4),
                Value::Text("paused".into()),
                Value::Text(format!("{schema}.state_domain_{levels}")),
            ],
            vec![
                Value::Int(5),
                Value::Null,
                Value::Text(format!("{schema}.state_domain_{levels}")),
            ],
        ]
    );

    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute(&format!("SET search_path TO {schema}, {shadow_schema}"))
        .await
        .unwrap();
    assert_eq!(
        transaction.query("SELECT current_schema()::text").await.unwrap().rows,
        vec![vec![Value::Text(schema.clone())]]
    );
    transaction
        .execute(&format!("SET search_path TO {shadow_schema}"))
        .await
        .unwrap();
    assert_eq!(
        transaction.query("SELECT current_schema()::text").await.unwrap().rows,
        vec![vec![Value::Text(shadow_schema.clone())]]
    );

    let (session_update, session_params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some(&schema),
        "rows",
        &columns,
        &[(1, Value::Text("paused".into()))],
        &[Value::Int(3)],
    )
    .unwrap();
    transaction
        .execute_params(&session_update, &session_params)
        .await
        .unwrap();
    let (session_insert, session_insert_params) = tablepro_core::sql_dialect::build_insert_from_draft(
        "postgres",
        Some(&schema),
        "rows",
        &columns,
        &[Value::Int(6), Value::Text("paused".into())],
    )
    .unwrap();
    transaction
        .execute_params(&session_insert, &session_insert_params)
        .await
        .unwrap();
    let (session_null_insert, session_null_params) = tablepro_core::sql_dialect::build_insert_from_draft(
        "postgres",
        Some(&schema),
        "rows",
        &columns,
        &[Value::Int(7), Value::Null],
    )
    .unwrap();
    transaction
        .execute_params(&session_null_insert, &session_null_params)
        .await
        .unwrap();

    let session_rows = transaction
        .query_params(
            &format!(
                "SELECT id, status::text, pg_typeof(status)::text \
                 FROM {schema}.rows WHERE {where_sql} ORDER BY id"
            ),
            &params,
        )
        .await
        .unwrap();
    let typed_paused = Value::Text(format!("{schema}.state_domain_{levels}"));
    assert_eq!(
        session_rows.rows,
        vec![
            vec![Value::Int(1), Value::Text("paused".into()), typed_paused.clone()],
            vec![Value::Int(3), Value::Text("paused".into()), typed_paused.clone()],
            vec![Value::Int(4), Value::Text("paused".into()), typed_paused.clone()],
            vec![Value::Int(6), Value::Text("paused".into()), typed_paused],
        ]
    );
    let session_null = transaction
        .query(&format!(
            "SELECT id, status IS NULL, pg_typeof(status)::text \
             FROM {schema}.rows WHERE id = 7"
        ))
        .await
        .unwrap();
    assert_eq!(
        session_null.rows,
        vec![vec![
            Value::Int(7),
            Value::Bool(true),
            Value::Text(format!("{schema}.state_domain_{levels}")),
        ]]
    );

    if levels >= 64 {
        let valid_raw = transaction
            .execute_params(
                &format!("UPDATE {schema}.rows SET status = $1 WHERE id = 3"),
                &[Value::Text("ready".into())],
            )
            .await
            .expect_err(&format!("raw inferred enum text must be refused at depth {levels}"));
        assert!(
            matches!(&valid_raw, tablepro_core::DriverError::Unsupported(message)
                if message.contains("resolvable depth")),
            "depth {levels}: {valid_raw:?}"
        );
    }

    let invalid = transaction
        .execute_params(
            &format!("UPDATE {schema}.rows SET status = $1 WHERE id = 3"),
            &[Value::Text("not a label".into())],
        )
        .await
        .expect_err(&format!(
            "{levels}-level target domain rejects invalid labels under shadowed search_path"
        ));
    if levels >= 64 {
        assert!(
            matches!(&invalid, tablepro_core::DriverError::Unsupported(message)
                if message.contains("resolvable depth")),
            "depth {levels}: {invalid:?}"
        );
    } else {
        assert!(
            matches!(
                &invalid,
                tablepro_core::DriverError::Query {
                    sqlstate: Some(code), ..
                } if code == "22P02"
            ),
            "depth {levels}: {invalid:?}"
        );
    }
    transaction.rollback().await.unwrap();

    let after_rollback = connection
        .query(&format!(
            "SELECT id, status::text, pg_typeof(status)::text FROM {schema}.rows ORDER BY id"
        ))
        .await
        .unwrap();
    assert_eq!(after_rollback.rows, stored.rows);
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
            FilterOp::NotEq,
            FilterValue::Single("NULL".into()),
            vec![(2, "ready"), (3, "東京")],
        ),
        (
            FilterOp::Lt,
            FilterValue::Single("東京".into()),
            vec![(1, "NULL"), (2, "ready")],
        ),
        (
            FilterOp::LtEq,
            FilterValue::Single("ready".into()),
            vec![(1, "NULL"), (2, "ready")],
        ),
        (FilterOp::Gt, FilterValue::Single("ready".into()), vec![(3, "東京")]),
        (
            FilterOp::GtEq,
            FilterValue::Single("ready".into()),
            vec![(2, "ready"), (3, "東京")],
        ),
        (
            FilterOp::Contains,
            FilterValue::Single("ead".into()),
            vec![(2, "ready")],
        ),
        (
            FilterOp::StartsWith,
            FilterValue::Single("re".into()),
            vec![(2, "ready")],
        ),
        (FilterOp::EndsWith, FilterValue::Single("dy".into()), vec![(2, "ready")]),
        (FilterOp::Like, FilterValue::Single("re%".into()), vec![(2, "ready")]),
        (
            FilterOp::NotLike,
            FilterValue::Single("東%".into()),
            vec![(1, "NULL"), (2, "ready")],
        ),
        (
            FilterOp::Ilike,
            FilterValue::Single("%READY%".into()),
            vec![(2, "ready")],
        ),
        (
            FilterOp::In,
            FilterValue::List(vec!["ready".into(), "東京".into()]),
            vec![(2, "ready"), (3, "東京")],
        ),
        (
            FilterOp::NotIn,
            FilterValue::List(vec!["NULL".into(), "東京".into()]),
            vec![(2, "ready")],
        ),
        (
            FilterOp::Between,
            FilterValue::Pair("NULL".into(), "ready".into()),
            vec![(1, "NULL"), (2, "ready")],
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

    let not_null_filter = FilterSet {
        rules: vec![FilterRule {
            column: "label".into(),
            op: FilterOp::IsNotNull,
            value: None,
        }],
        ..Default::default()
    };
    let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &not_null_filter)
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
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("NULL".into()),
                Value::Text("value_contract_domain_filter.status_domain".into())
            ],
            vec![
                Value::Int(2),
                Value::Text("ready".into()),
                Value::Text("value_contract_domain_filter.status_domain".into())
            ],
            vec![
                Value::Int(3),
                Value::Text("東京".into()),
                Value::Text("value_contract_domain_filter.status_domain".into())
            ],
        ]
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

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_sql_file_replay_preserves_values_and_type() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    for sql in [
        "CREATE SCHEMA value_contract_domain_sql",
        "CREATE TYPE value_contract_domain_sql.state AS ENUM ('NULL', '', '東京', 'x''; DROP TABLE keep_me; --')",
        "CREATE DOMAIN value_contract_domain_sql.state_domain AS value_contract_domain_sql.state",
        "CREATE TABLE value_contract_domain_sql.source_rows (id INT PRIMARY KEY, label value_contract_domain_sql.state_domain)",
        "CREATE TABLE value_contract_domain_sql.restore_rows (id INT PRIMARY KEY, label value_contract_domain_sql.state_domain)",
        "INSERT INTO value_contract_domain_sql.source_rows VALUES \
         (1, 'NULL'), (2, ''), (3, '東京'), (4, 'x''; DROP TABLE keep_me; --'), (5, NULL)",
    ] {
        connection.execute(sql).await.unwrap();
    }
    connection
        .execute("CREATE TABLE value_contract_domain_sql.keep_me (id INT PRIMARY KEY)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO value_contract_domain_sql.keep_me VALUES (1)")
        .await
        .unwrap();

    let result = connection
        .query("SELECT id, label FROM value_contract_domain_sql.source_rows ORDER BY id")
        .await
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("domain-enum.sql");
    tablepro_core::export::write_result_file(
        &path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Sql,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: Some(tablepro_core::export::SqlTarget {
                driver_id: "postgres",
                schema: Some("value_contract_domain_sql"),
                table: "restore_rows",
            }),
        },
        || false,
        |_| {},
    )
    .unwrap();
    let sql = std::fs::read_to_string(&path).unwrap();
    assert_eq!(sql.lines().count(), result.rows.len());
    assert!(
        sql.lines()
            .all(|line| line.starts_with("INSERT INTO \"value_contract_domain_sql\".\"restore_rows\""))
    );
    for statement in sql.lines() {
        connection.execute(statement).await.unwrap();
    }

    let restored = connection
        .query(
            "SELECT id, label::text, pg_typeof(label)::text \
             FROM value_contract_domain_sql.restore_rows ORDER BY id",
        )
        .await
        .unwrap();
    let domain_type = Value::Text("value_contract_domain_sql.state_domain".into());
    assert_eq!(
        restored.rows,
        vec![
            vec![Value::Int(1), Value::Text("NULL".into()), domain_type.clone()],
            vec![Value::Int(2), Value::Text(String::new()), domain_type.clone()],
            vec![Value::Int(3), Value::Text("東京".into()), domain_type.clone()],
            vec![
                Value::Int(4),
                Value::Text("x'; DROP TABLE keep_me; --".into()),
                domain_type.clone(),
            ],
            vec![Value::Int(5), Value::Null, domain_type],
        ]
    );
    let table = connection
        .query("SELECT count(*)::bigint FROM value_contract_domain_sql.keep_me")
        .await
        .unwrap();
    assert_eq!(table.rows, vec![vec![Value::Int(1)]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_file_formats_preserve_values_and_refuse_empty_xlsx() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_domain_formats")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_domain_formats.label AS ENUM \
             ('NULL', '', '東京', '</label><img src=x onerror=\"alert(''x'')\">', \
              E'a|b\\nc', '<tag>&amp;', '=1+1')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_domain_formats.label_domain \
             AS value_contract_domain_formats.label",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_domain_formats.rows \
             (id INT PRIMARY KEY, label value_contract_domain_formats.label_domain)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_domain_formats.rows VALUES \
             (1, 'NULL'), (2, ''), (3, '東京'), \
             (4, '</label><img src=x onerror=\"alert(''x'')\">'), \
             (5, E'a|b\\nc'), (6, '<tag>&amp;'), (7, '=1+1'), (8, NULL)",
        )
        .await
        .unwrap();

    let all_rows = connection
        .query(
            "SELECT id, label, pg_typeof(label)::text AS native_type \
             FROM value_contract_domain_formats.rows ORDER BY id",
        )
        .await
        .unwrap();
    let domain_type = Value::Text("value_contract_domain_formats.label_domain".into());
    let hostile = "</label><img src=x onerror=\"alert('x')\">";
    assert_eq!(
        all_rows.rows,
        vec![
            vec![Value::Int(1), Value::Text("NULL".into()), domain_type.clone()],
            vec![Value::Int(2), Value::Text(String::new()), domain_type.clone()],
            vec![Value::Int(3), Value::Text("東京".into()), domain_type.clone()],
            vec![Value::Int(4), Value::Text(hostile.into()), domain_type.clone()],
            vec![Value::Int(5), Value::Text("a|b\nc".into()), domain_type.clone()],
            vec![Value::Int(6), Value::Text("<tag>&amp;".into()), domain_type.clone()],
            vec![Value::Int(7), Value::Text("=1+1".into()), domain_type.clone()],
            vec![Value::Int(8), Value::Null, domain_type],
        ]
    );

    let directory = tempfile::tempdir().unwrap();
    let csv = tablepro_core::export::CsvOptions::default();
    for (name, format) in [
        ("domain.xml", tablepro_core::export::ResultFormat::Xml),
        ("domain.html", tablepro_core::export::ResultFormat::Html),
        ("domain.md", tablepro_core::export::ResultFormat::Markdown),
    ] {
        let path = directory.path().join(name);
        tablepro_core::export::write_result_file(
            &path,
            &all_rows,
            &tablepro_core::export::ResultExport {
                format,
                csv: &csv,
                sql: None,
            },
            || false,
            |_| {},
        )
        .unwrap();
        let output = std::fs::read_to_string(&path).unwrap();
        match format {
            tablepro_core::export::ResultFormat::Xml => {
                assert!(output.contains("<label>NULL</label>"), "{output}");
                assert!(output.contains("<label></label>"), "{output}");
                assert!(output.contains("<label>東京</label>"), "{output}");
                assert!(output.contains("onerror=&quot;alert(&apos;x&apos;)&quot;"), "{output}");
                assert!(output.contains("<label null=\"true\"/>"), "{output}");
            }
            tablepro_core::export::ResultFormat::Html => {
                assert!(output.contains("<td>NULL</td>"), "{output}");
                assert!(output.contains("<td></td>"), "{output}");
                assert!(output.contains("<td>東京</td>"), "{output}");
                assert!(output.contains("&lt;/label&gt;&lt;img src=x"), "{output}");
                assert!(!output.contains("<img src=x"), "{output}");
                assert!(output.contains("<td class=\"null\"></td>"), "{output}");
            }
            tablepro_core::export::ResultFormat::Markdown => {
                assert!(output.contains("| \"NULL\" |"), "{output}");
                assert!(output.contains("| \"\" |"), "{output}");
                assert!(output.contains("| \"東京\" |"), "{output}");
                assert!(output.contains("a\\|b&#92;nc"), "{output}");
                assert!(output.contains("&lt;tag&gt;&amp;amp;"), "{output}");
                assert!(output.contains("| NULL |"), "{output}");
            }
            tablepro_core::export::ResultFormat::Csv
            | tablepro_core::export::ResultFormat::Json
            | tablepro_core::export::ResultFormat::Sql
            | tablepro_core::export::ResultFormat::Xlsx => {}
        }
    }

    let valid_rows = connection
        .query(
            "SELECT id, label, pg_typeof(label)::text AS native_type \
             FROM value_contract_domain_formats.rows WHERE label::text <> '' ORDER BY id",
        )
        .await
        .unwrap();
    let xlsx_path = directory.path().join("domain.xlsx");
    tablepro_core::export::write_result_file(
        &xlsx_path,
        &valid_rows,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &csv,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let workbook = std::fs::read(&xlsx_path).unwrap();
    assert!(workbook.starts_with(b"PK"));
    assert!(workbook.len() > 1_000, "{}", workbook.len());

    let refusal_path = directory.path().join("keep.xlsx");
    std::fs::write(&refusal_path, b"existing workbook").unwrap();
    let error = tablepro_core::export::write_result_file(
        &refusal_path,
        &all_rows,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &csv,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap_err();
    assert!(
        matches!(
            error,
            tablepro_core::export::ExportError::WorkbookEmptyText { row: 2, column: 2 }
        ),
        "{error:?}"
    );
    assert_eq!(std::fs::read(&refusal_path).unwrap(), b"existing workbook");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 5);
}
