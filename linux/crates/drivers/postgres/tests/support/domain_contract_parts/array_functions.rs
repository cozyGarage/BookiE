#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_array_functions_infer_parameter_type() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_domain_array_functions")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_domain_array_functions.state \
             AS ENUM ('ready', 'paused')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_domain_array_functions.state_domain \
             AS value_contract_domain_array_functions.state",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_domain_array_functions.rows \
             (id INT PRIMARY KEY, status value_contract_domain_array_functions.state_domain)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_domain_array_functions.rows VALUES \
             (1, 'ready'), (2, 'paused'), (3, NULL)",
        )
        .await
        .unwrap();

    for (expression, text_values, null_values) in [
        (
            "array_append(ARRAY[status], $1)",
            [
                r#"["ready","paused"]"#,
                r#"["paused","paused"]"#,
                r#"[null,"paused"]"#,
            ],
            [r#"["ready",null]"#, r#"["paused",null]"#, "[null,null]"],
        ),
        (
            "array_prepend($1, ARRAY[status])",
            [
                r#"["paused","ready"]"#,
                r#"["paused","paused"]"#,
                r#"["paused",null]"#,
            ],
            [r#"[null,"ready"]"#, r#"[null,"paused"]"#, "[null,null]"],
        ),
    ] {
        for (parameter, expected) in [
            (
                Value::Text("paused".into()),
                text_values.map(str::to_owned),
            ),
            (Value::Null, null_values.map(str::to_owned)),
        ] {
            let result = connection
                .query_params(
                    &format!(
                        "SELECT id, array_to_json({expression})::text, \
                         pg_typeof($1)::text, pg_typeof(ARRAY[status])::text, \
                         pg_typeof({expression})::text, pg_typeof(status)::text \
                         FROM value_contract_domain_array_functions.rows ORDER BY id"
                    ),
                    std::slice::from_ref(&parameter),
                )
                .await
                .unwrap();
            assert_eq!(
                result.rows,
                expected
                    .into_iter()
                    .enumerate()
                    .map(|(index, values)| vec![
                        Value::Int(index as i64 + 1),
                        Value::Text(values),
                        Value::Text("value_contract_domain_array_functions.state".into()),
                        Value::Text("value_contract_domain_array_functions.state_domain[]".into()),
                        Value::Text("value_contract_domain_array_functions.state[]".into()),
                        Value::Text("value_contract_domain_array_functions.state_domain".into()),
                    ])
                    .collect::<Vec<_>>(),
                "{expression} with parameter {parameter:?}"
            );
        }
    }
}
