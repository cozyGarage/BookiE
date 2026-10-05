#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{DriverError, Value};

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_enum_parameters_in_coalesce_array_append_and_nullif() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    connection
        .execute("CREATE SCHEMA value_contract_enum_parameter_context")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_enum_parameter_context.state \
             AS ENUM ('ready', 'paused', 'NULL', '')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_enum_parameter_context.rows \
             (id INT PRIMARY KEY, state value_contract_enum_parameter_context.state)",
        )
        .await
        .unwrap();
    connection
        .execute("INSERT INTO value_contract_enum_parameter_context.rows VALUES (1, 'ready'), (2, NULL)")
        .await
        .unwrap();

    let enum_type = "value_contract_enum_parameter_context.state";
    for (parameter, expected_coalesced, expected_array, expected_fallback) in [
        (
            Value::Text("NULL".into()),
            "NULL",
            r#"["ready","NULL"]"#,
            Value::Text("NULL".into()),
        ),
        (Value::Null, "ready", "[\"ready\",null]", Value::Null),
    ] {
        let coalesced = connection
            .query_params(
                "SELECT COALESCE($1, state)::text, pg_typeof(COALESCE($1, state))::text \
                 FROM value_contract_enum_parameter_context.rows WHERE id = 1",
                std::slice::from_ref(&parameter),
            )
            .await
            .unwrap();
        assert_eq!(
            coalesced.rows,
            vec![vec![
                Value::Text(expected_coalesced.into()),
                Value::Text(enum_type.into()),
            ]]
        );

        let fallback = connection
            .query_params(
                "SELECT COALESCE(state, $1)::text, pg_typeof(COALESCE(state, $1))::text \
                 FROM value_contract_enum_parameter_context.rows WHERE id = 2",
                std::slice::from_ref(&parameter),
            )
            .await
            .unwrap();
        assert_eq!(
            fallback.rows,
            vec![vec![expected_fallback, Value::Text(enum_type.into())]]
        );

        let appended = connection
            .query_params(
                "SELECT array_to_json(array_append(ARRAY[state], $1))::text, \
                        pg_typeof(array_append(ARRAY[state], $1))::text \
                 FROM value_contract_enum_parameter_context.rows WHERE id = 1",
                std::slice::from_ref(&parameter),
            )
            .await
            .unwrap();
        assert_eq!(
            appended.rows,
            vec![vec![
                Value::Text(expected_array.into()),
                Value::Text(format!("{enum_type}[]")),
            ]]
        );
    }

    for (parameter, native_parameter) in [
        (Value::Text("NULL".into()), "'NULL'"),
        (Value::Text(String::new()), "''"),
        (Value::Null, "NULL"),
    ] {
        let native = connection
            .query(&format!(
                "SELECT id, \
                 CASE WHEN id = 1 THEN {native_parameter}::{enum_type} ELSE state END::text, \
                 pg_typeof(CASE WHEN id = 1 THEN {native_parameter}::{enum_type} ELSE state END)::text, \
                 CASE WHEN id = 1 THEN state ELSE {native_parameter}::{enum_type} END::text, \
                 pg_typeof(CASE WHEN id = 1 THEN state ELSE {native_parameter}::{enum_type} END)::text \
                 FROM value_contract_enum_parameter_context.rows ORDER BY id"
            ))
            .await
            .unwrap();
        let inferred = connection
            .query_params(
                "SELECT id, \
                 CASE WHEN id = 1 THEN $1 ELSE state END::text, pg_typeof($1)::text, \
                 pg_typeof(CASE WHEN id = 1 THEN $1 ELSE state END)::text, \
                 CASE WHEN id = 1 THEN state ELSE $1 END::text, \
                 pg_typeof(CASE WHEN id = 1 THEN state ELSE $1 END)::text \
                 FROM value_contract_enum_parameter_context.rows ORDER BY id",
                std::slice::from_ref(&parameter),
            )
            .await
            .unwrap();
        let expected = native
            .rows
            .into_iter()
            .map(|row| {
                vec![
                    row[0].clone(),
                    row[1].clone(),
                    Value::Text(enum_type.into()),
                    row[2].clone(),
                    row[3].clone(),
                    row[4].clone(),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(inferred.rows, expected, "CASE enum parameter {parameter:?}");

        for (native_expression, inferred_expression) in [
            (
                format!("GREATEST({native_parameter}::{enum_type}, state)"),
                "GREATEST($1, state)",
            ),
            (
                format!("GREATEST(state, {native_parameter}::{enum_type})"),
                "GREATEST(state, $1)",
            ),
            (
                format!("LEAST({native_parameter}::{enum_type}, state)"),
                "LEAST($1, state)",
            ),
            (
                format!("LEAST(state, {native_parameter}::{enum_type})"),
                "LEAST(state, $1)",
            ),
        ] {
            let native = connection
                .query(&format!(
                    "SELECT id, {native_expression}::text, \
                     pg_typeof({native_expression})::text \
                     FROM value_contract_enum_parameter_context.rows ORDER BY id"
                ))
                .await
                .unwrap();
            let inferred = connection
                .query_params(
                    &format!(
                        "SELECT id, {inferred_expression}::text, pg_typeof($1)::text, \
                         pg_typeof({inferred_expression})::text \
                         FROM value_contract_enum_parameter_context.rows ORDER BY id"
                    ),
                    std::slice::from_ref(&parameter),
                )
                .await
                .unwrap();
            let expected = native
                .rows
                .into_iter()
                .map(|row| {
                    vec![
                        row[0].clone(),
                        row[1].clone(),
                        Value::Text(enum_type.into()),
                        row[2].clone(),
                    ]
                })
                .collect::<Vec<_>>();
            assert_eq!(inferred.rows, expected, "{inferred_expression} with {parameter:?}");
        }
    }

    for (parameter, expected_ready) in [
        (Value::Text("ready".into()), Value::Null),
        (Value::Text("paused".into()), Value::Text("ready".into())),
        (Value::Null, Value::Text("ready".into())),
    ] {
        let compared = connection
            .query_params(
                "SELECT id, NULLIF(state, $1)::text, \
                        pg_typeof(NULLIF(state, $1))::text, pg_typeof($1)::text \
                 FROM value_contract_enum_parameter_context.rows ORDER BY id",
                std::slice::from_ref(&parameter),
            )
            .await
            .unwrap();
        assert_eq!(
            compared.rows,
            vec![
                vec![
                    Value::Int(1),
                    expected_ready,
                    Value::Text(enum_type.into()),
                    Value::Text(enum_type.into()),
                ],
                vec![
                    Value::Int(2),
                    Value::Null,
                    Value::Text(enum_type.into()),
                    Value::Text(enum_type.into()),
                ],
            ],
            "NULLIF parameter {parameter:?}"
        );
    }

    for sql in [
        "SELECT COALESCE($1, state) FROM value_contract_enum_parameter_context.rows WHERE id = 1",
        "SELECT NULLIF(state, $1) FROM value_contract_enum_parameter_context.rows WHERE id = 1",
        "SELECT array_append(ARRAY[state], $1) FROM value_contract_enum_parameter_context.rows WHERE id = 1",
        "SELECT CASE WHEN id = 1 THEN $1 ELSE state END FROM value_contract_enum_parameter_context.rows",
        "SELECT CASE WHEN id = 1 THEN state ELSE $1 END FROM value_contract_enum_parameter_context.rows",
        "SELECT GREATEST($1, state) FROM value_contract_enum_parameter_context.rows",
        "SELECT GREATEST(state, $1) FROM value_contract_enum_parameter_context.rows",
        "SELECT LEAST($1, state) FROM value_contract_enum_parameter_context.rows",
        "SELECT LEAST(state, $1) FROM value_contract_enum_parameter_context.rows",
    ] {
        let error = connection
            .query_params(sql, &[Value::Text("not-a-label".into())])
            .await
            .expect_err("invalid enum text must reach PostgreSQL's enum input validation");
        assert!(
            matches!(&error, DriverError::Query { sqlstate: Some(code), .. } if code == "22P02"),
            "expected native invalid-enum SQLSTATE 22P02, got {error:?}"
        );
    }

    let unchanged = connection
        .query(
            "SELECT id, state::text, pg_typeof(state)::text \
             FROM value_contract_enum_parameter_context.rows ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        unchanged.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("ready".into()),
                Value::Text(enum_type.into())
            ],
            vec![Value::Int(2), Value::Null, Value::Text(enum_type.into())],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_enum_parameters_resolve_target_under_shadowed_search_path() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    let enum_schema = "value_contract_enum_shadowed_context";
    let shadow_schema = "value_contract_enum_shadowed_context_shadow";
    let enum_type = format!("{enum_schema}.state");
    connection
        .execute(&format!("CREATE SCHEMA {enum_schema}"))
        .await
        .unwrap();
    connection
        .execute(&format!("CREATE SCHEMA {shadow_schema}"))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "CREATE TYPE {enum_type} AS ENUM ('ready', 'paused', 'NULL', '')"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "CREATE TYPE {shadow_schema}.state AS ENUM ('ready', 'shadow-only')"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "CREATE TABLE {enum_schema}.rows \
             (id INT PRIMARY KEY, state {enum_type})"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "INSERT INTO {enum_schema}.rows VALUES \
             (1, 'ready'), (2, 'paused'), (3, 'NULL'), (4, ''), (5, NULL)"
        ))
        .await
        .unwrap();

    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute(&format!("SET LOCAL search_path TO {shadow_schema}, public"))
        .await
        .unwrap();

    let expressions = [
        ("COALESCE({p}::{t}, state)", "COALESCE($1, state)"),
        ("COALESCE(state, {p}::{t})", "COALESCE(state, $1)"),
        ("GREATEST({p}::{t}, state)", "GREATEST($1, state)"),
        ("GREATEST(state, {p}::{t})", "GREATEST(state, $1)"),
        ("LEAST({p}::{t}, state)", "LEAST($1, state)"),
        ("LEAST(state, {p}::{t})", "LEAST(state, $1)"),
    ];
    let parameters = [
        (Value::Text("paused".into()), "'paused'"),
        (Value::Text("NULL".into()), "'NULL'"),
        (Value::Text(String::new()), "''"),
        (Value::Null, "NULL"),
    ];

    for (parameter, native_parameter) in &parameters {
        for (native_template, inferred_expression) in expressions {
            let native_expression = native_template
                .replace("{p}", native_parameter)
                .replace("{t}", &enum_type);
            let native = transaction
                .query(&format!(
                    "SELECT id, {native_expression}::text, \
                     pg_typeof({native_parameter}::{enum_type})::text, \
                     pg_typeof({native_expression})::text, pg_typeof(state)::text \
                     FROM {enum_schema}.rows ORDER BY id"
                ))
                .await
                .unwrap();
            let inferred = transaction
                .query_params(
                    &format!(
                        "SELECT id, {inferred_expression}::text, pg_typeof($1)::text, \
                         pg_typeof({inferred_expression})::text, pg_typeof(state)::text \
                         FROM {enum_schema}.rows ORDER BY id"
                    ),
                    std::slice::from_ref(parameter),
                )
                .await
                .unwrap();
            let expected = native
                .rows
                .into_iter()
                .map(|row| {
                    vec![
                        row[0].clone(),
                        row[1].clone(),
                        row[2].clone(),
                        row[3].clone(),
                        row[4].clone(),
                    ]
                })
                .collect::<Vec<_>>();
            assert_eq!(
                inferred.rows, expected,
                "{inferred_expression} with parameter {parameter:?}"
            );
        }
    }

    for (_, inferred_expression) in expressions {
        transaction
            .execute("SAVEPOINT invalid_shadowed_enum_parameter")
            .await
            .unwrap();
        let invalid = transaction
            .query_params(
                &format!("SELECT {inferred_expression} FROM {enum_schema}.rows ORDER BY id"),
                &[Value::Text("shadow-only".into())],
            )
            .await
            .expect_err("inferred enum parameters must resolve against the column's schema");
        assert!(
            matches!(&invalid, DriverError::Query { sqlstate: Some(code), .. } if code == "22P02"),
            "expected target enum invalid-label SQLSTATE 22P02, got {invalid:?}"
        );
        transaction
            .execute("ROLLBACK TO SAVEPOINT invalid_shadowed_enum_parameter")
            .await
            .unwrap();
    }

    let unchanged = transaction
        .query(&format!(
            "SELECT id, state::text, pg_typeof(state)::text \
             FROM {enum_schema}.rows ORDER BY id"
        ))
        .await
        .unwrap();
    assert_eq!(
        unchanged.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("ready".into()),
                Value::Text(enum_type.clone())
            ],
            vec![
                Value::Int(2),
                Value::Text("paused".into()),
                Value::Text(enum_type.clone())
            ],
            vec![
                Value::Int(3),
                Value::Text("NULL".into()),
                Value::Text(enum_type.clone())
            ],
            vec![
                Value::Int(4),
                Value::Text(String::new()),
                Value::Text(enum_type.clone())
            ],
            vec![Value::Int(5), Value::Null, Value::Text(enum_type)],
        ]
    );
}
