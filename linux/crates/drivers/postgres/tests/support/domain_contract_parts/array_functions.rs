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

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_enum_array_remove_infers_scalar_parameter_type() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_domain_array_remove")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_domain_array_remove.state \
             AS ENUM ('ready', 'paused')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_domain_array_remove.state_domain \
             AS value_contract_domain_array_remove.state",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_domain_array_remove.rows \
             (id INT PRIMARY KEY, status value_contract_domain_array_remove.state_domain)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_domain_array_remove.rows VALUES \
             (1, 'ready'), (2, 'paused'), (3, NULL)",
        )
        .await
        .unwrap();

    let array = "ARRAY[status::value_contract_domain_array_remove.state]";
    for (parameter, native_value) in [
        (
            Value::Text("paused".into()),
            "'paused'::value_contract_domain_array_remove.state",
        ),
        (Value::Null, "NULL::value_contract_domain_array_remove.state"),
    ] {
        let native = connection
            .query(&format!(
                "SELECT id, array_to_json(array_remove({array}, {native_value}))::text \
                 FROM value_contract_domain_array_remove.rows ORDER BY id"
            ))
            .await
            .unwrap();
        let result = connection
            .query_params(
                &format!(
                    "SELECT id, array_to_json(array_remove({array}, $1))::text, \
                     pg_typeof($1)::text, pg_typeof(array_remove({array}, $1))::text \
                     FROM value_contract_domain_array_remove.rows ORDER BY id"
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
                    Value::Text("value_contract_domain_array_remove.state".into()),
                    Value::Text("value_contract_domain_array_remove.state[]".into()),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(result.rows, expected, "array_remove with {parameter:?}");
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_enum_array_position_infers_scalar_parameter_and_matches_null() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_domain_array_position")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_domain_array_position.state \
             AS ENUM ('ready', 'paused')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_domain_array_position.state_domain \
             AS value_contract_domain_array_position.state",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_domain_array_position.rows \
             (id INT PRIMARY KEY, status value_contract_domain_array_position.state_domain)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_domain_array_position.rows VALUES \
             (1, 'ready'), (2, 'paused'), (3, NULL)",
        )
        .await
        .unwrap();

    let array = "ARRAY[status::value_contract_domain_array_position.state]";
    for (parameter, native_value) in [
        (
            Value::Text("paused".into()),
            "'paused'::value_contract_domain_array_position.state",
        ),
        (
            Value::Null,
            "NULL::value_contract_domain_array_position.state",
        ),
    ] {
        let native = connection
            .query(&format!(
                "SELECT id, array_position({array}, {native_value}) \
                 FROM value_contract_domain_array_position.rows ORDER BY id"
            ))
            .await
            .unwrap();
        let result = connection
            .query_params(
                &format!(
                    "SELECT id, array_position({array}, $1), pg_typeof($1)::text, \
                     pg_typeof(array_position({array}, $1))::text \
                     FROM value_contract_domain_array_position.rows ORDER BY id"
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
                    Value::Text("value_contract_domain_array_position.state".into()),
                    Value::Text("integer".into()),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(result.rows, expected, "array_position with {parameter:?}");
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_any_infers_array_parameter_type() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_domain_any_array")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_domain_any_array.state \
             AS ENUM ('ready', 'paused', 'NULL', '', 'comma,label', 'quote\"label', 'backslash\\label')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_domain_any_array.state_domain \
             AS value_contract_domain_any_array.state",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_domain_any_array.rows \
             (id INT PRIMARY KEY, status value_contract_domain_any_array.state_domain)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_domain_any_array.rows VALUES \
             (1, 'ready'), (2, 'paused'), (3, 'NULL'), (4, ''), (5, NULL), \
             (6, 'comma,label'), (7, 'quote\"label'), (8, 'backslash\\label')",
        )
        .await
        .unwrap();

    let domain_type = "value_contract_domain_any_array.state_domain";
    let enum_array_type = "value_contract_domain_any_array.state[]";
    for (parameter, native_array, expected) in [
        (
            Value::Text(r#"{"ready","paused"}"#.into()),
            r#"'{"ready","paused"}'::value_contract_domain_any_array.state[]"#,
            [
                Value::Bool(true),
                Value::Bool(true),
                Value::Bool(false),
                Value::Bool(false),
                Value::Null,
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
            ],
        ),
        (
            Value::Text(r#"{"NULL",""}"#.into()),
            r#"'{"NULL",""}'::value_contract_domain_any_array.state[]"#,
            [
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(true),
                Value::Bool(true),
                Value::Null,
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
            ],
        ),
        (
            Value::Text(r#"{"comma,label","quote\"label","backslash\\label"}"#.into()),
            r#"'{"comma,label","quote\"label","backslash\\label"}'::value_contract_domain_any_array.state[]"#,
            [
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::Null,
                Value::Bool(true),
                Value::Bool(true),
                Value::Bool(true),
            ],
        ),
        (
            Value::Text(r#"[0:1][3:4]={{"ready","NULL"},{"",NULL}}"#.into()),
            r#"'[0:1][3:4]={{"ready","NULL"},{"",NULL}}'::value_contract_domain_any_array.state[]"#,
            [
                Value::Bool(true),
                Value::Null,
                Value::Bool(true),
                Value::Bool(true),
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
            ],
        ),
        (
            Value::Text("{}".into()),
            "'{}'::value_contract_domain_any_array.state[]",
            [
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
            ],
        ),
        (
            Value::Null,
            "NULL::value_contract_domain_any_array.state[]",
            [
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
            ],
        ),
    ] {
        let native_wire = connection
            .query(&format!(
                "SELECT encode(array_send({native_array}), 'hex')"
            ))
            .await
            .unwrap()
            .rows[0][0]
            .clone();
        let result = connection
            .query_params(
                "SELECT id, status::value_contract_domain_any_array.state = ANY($1), \
                 pg_typeof($1)::text, pg_typeof(status)::text, encode(array_send($1), 'hex') \
                 FROM value_contract_domain_any_array.rows ORDER BY id",
                std::slice::from_ref(&parameter),
            )
            .await
            .unwrap();
        assert_eq!(
            result.rows,
            expected
                .into_iter()
                .enumerate()
                .map(|(index, value)| vec![
                    Value::Int(index as i64 + 1),
                    value,
                    Value::Text(enum_array_type.into()),
                    Value::Text(domain_type.into()),
                    native_wire.clone(),
                ])
                .collect::<Vec<_>>(),
            "ANY with parameter {parameter:?}"
        );
    }

    let invalid = connection
        .query_params(
            "SELECT status::value_contract_domain_any_array.state = ANY($1) \
             FROM value_contract_domain_any_array.rows WHERE id = 1",
            &[Value::Text(r#"{"not-a-label"}"#.into())],
        )
        .await
        .expect_err("invalid array enum labels must be refused by PostgreSQL");
    assert!(
        matches!(&invalid, tablepro_core::DriverError::Query { sqlstate: Some(code), .. } if code == "22P02"),
        "expected native invalid-enum SQLSTATE 22P02, got {invalid:?}"
    );

    let invalid_bounds = connection
        .query_params(
            "SELECT status::value_contract_domain_any_array.state = ANY($1) \
             FROM value_contract_domain_any_array.rows WHERE id = 1",
            &[Value::Text(r#"[0:1]={ready}"#.into())],
        )
        .await
        .expect_err("array bounds inconsistent with the values must be refused before dispatch");
    assert!(
        matches!(&invalid_bounds, tablepro_core::DriverError::Unsupported(message) if message.contains("bounds")),
        "expected an explicit unsupported result for inconsistent bounds, got {invalid_bounds:?}"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_array_operators_infer_parameter_type_in_both_positions() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_domain_array_operators")
        .await
        .unwrap();
    connection
        .execute("CREATE SCHEMA value_contract_domain_array_operator_shadow")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_domain_array_operator_shadow.state \
             AS ENUM ('shadow-only')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TYPE value_contract_domain_array_operators.state AS ENUM \
             ('ready', 'paused', 'NULL', '', 'comma,label', 'quote\"label', 'backslash\\label')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE DOMAIN value_contract_domain_array_operators.state_domain \
             AS value_contract_domain_array_operators.state",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_domain_array_operators.rows \
             (id INT PRIMARY KEY, status value_contract_domain_array_operators.state_domain)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_domain_array_operators.rows VALUES \
             (1, 'ready'), (2, 'paused'), (3, 'NULL'), (4, ''), (5, NULL), \
             (6, 'comma,label'), (7, 'quote\"label'), (8, 'backslash\\label')",
        )
        .await
        .unwrap();

    let schema = "value_contract_domain_array_operators";
    let domain_type = format!("{schema}.state_domain");
    let array_type = format!("{schema}.state[]");
    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute("SET LOCAL search_path TO value_contract_domain_array_operator_shadow, public")
        .await
        .unwrap();
    let cases = [
        (
            Value::Text(r#"{"ready","paused"}"#.into()),
            r#"'{"ready","paused"}'::value_contract_domain_array_operators.state[]"#,
        ),
        (
            Value::Text(r#"{"NULL",""}"#.into()),
            r#"'{"NULL",""}'::value_contract_domain_array_operators.state[]"#,
        ),
        (
            Value::Text(r#"{"comma,label","quote\"label","backslash\\label"}"#.into()),
            r#"'{"comma,label","quote\"label","backslash\\label"}'::value_contract_domain_array_operators.state[]"#,
        ),
        (
            Value::Text("{}".into()),
            "'{}'::value_contract_domain_array_operators.state[]",
        ),
        (
            Value::Null,
            "NULL::value_contract_domain_array_operators.state[]",
        ),
    ];

    for expression in [
        "ARRAY[status::value_contract_domain_array_operators.state] <@ $1",
        "$1 @> ARRAY[status::value_contract_domain_array_operators.state]",
        "ARRAY[status::value_contract_domain_array_operators.state] && $1",
        "$1 && ARRAY[status::value_contract_domain_array_operators.state]",
    ] {
        for (parameter, native_array) in cases.iter() {
            let native_expression = expression.replace("$1", native_array);
            let oracle = transaction
                .query(&format!(
                    "SELECT id, {native_expression} FROM \
                     value_contract_domain_array_operators.rows ORDER BY id"
                ))
                .await
                .unwrap();
            let wire = transaction
                .query(&format!("SELECT encode(array_send({native_array}), 'hex')"))
                .await
                .unwrap()
                .rows[0][0]
                .clone();
            let result = transaction
                .query_params(
                    &format!(
                        "SELECT id, {expression}, pg_typeof($1)::text, \
                         pg_typeof(status)::text, encode(array_send($1), 'hex') \
                         FROM value_contract_domain_array_operators.rows ORDER BY id"
                    ),
                    std::slice::from_ref(parameter),
                )
                .await
                .unwrap();
            let expected = oracle
                .rows
                .into_iter()
                .map(|row| {
                    vec![
                        row[0].clone(),
                        row[1].clone(),
                        Value::Text(array_type.clone()),
                        Value::Text(domain_type.clone()),
                        wire.clone(),
                    ]
                })
                .collect::<Vec<_>>();
            assert_eq!(result.rows, expected, "{expression} with {parameter:?}");
        }
    }

    for expression in [
        "ARRAY[status::value_contract_domain_array_operators.state] <@ $1",
        "$1 @> ARRAY[status::value_contract_domain_array_operators.state]",
    ] {
        transaction.execute("SAVEPOINT invalid_enum_parameter").await.unwrap();
        let invalid = transaction
            .query_params(
                &format!(
                    "SELECT {expression} FROM \
                     value_contract_domain_array_operators.rows WHERE id = 1"
                ),
                &[Value::Text(r#"{"missing-label"}"#.into())],
            )
            .await
            .expect_err("invalid array enum labels must retain PostgreSQL's native error");
        assert!(
            matches!(&invalid, tablepro_core::DriverError::Query { sqlstate: Some(code), .. } if code == "22P02"),
            "expected native invalid-enum SQLSTATE 22P02, got {invalid:?}"
        );
        transaction
            .execute("ROLLBACK TO SAVEPOINT invalid_enum_parameter")
            .await
            .unwrap();
    }
}
