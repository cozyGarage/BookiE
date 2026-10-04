#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::Value;

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_enum_parameters_infer_in_coalesce_and_array_append() {
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
