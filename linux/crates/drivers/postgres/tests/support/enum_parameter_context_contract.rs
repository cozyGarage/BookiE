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
