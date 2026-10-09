async fn assert_deep_enum_expression_parameters(
    connection: &dyn tablepro_core::Connection,
    schema: &str,
    levels: usize,
) {
    for (parameter, expected) in [
        (
            Value::Text("paused".into()),
            [Some("ready"), Some("paused"), Some("ready")],
        ),
        (Value::Null, [Some("ready"), None, Some("ready")]),
    ] {
        let result = connection
            .query_params(
                &format!(
                    "SELECT id, COALESCE(status, $1)::text, pg_typeof($1)::text, \
                     pg_typeof(COALESCE(status, $1))::text FROM {schema}.rows ORDER BY id"
                ),
                std::slice::from_ref(&parameter),
            )
            .await
            .unwrap_or_else(|error| panic!("COALESCE, depth {levels}: {error:?}"));
        assert_eq!(
            result.rows,
            expected
                .into_iter()
                .enumerate()
                .map(|(index, value)| vec![
                    Value::Int(index as i64 + 1),
                    value.map_or(Value::Null, |value| Value::Text(value.into())),
                    Value::Text(format!("{schema}.state")),
                    Value::Text(format!("{schema}.state")),
                ])
                .collect::<Vec<_>>(),
            "COALESCE parameter {parameter:?}, depth {levels}"
        );
    }
    assert_deep_enum_case_parameters(connection, schema, levels).await;
    assert_deep_enum_array_expression_refused(connection, schema, levels).await;
}

async fn assert_deep_enum_case_parameters(
    connection: &dyn tablepro_core::Connection,
    schema: &str,
    levels: usize,
) {
    for (parameter, cases) in [
        (
            Value::Text("paused".into()),
            [
                ("CASE WHEN id = 1 THEN $1 ELSE status END", [Some("paused"), None, Some("ready")]),
                ("CASE WHEN id = 1 THEN status ELSE $1 END", [Some("ready"), Some("paused"), Some("paused")]),
            ],
        ),
        (
            Value::Null,
            [
                ("CASE WHEN id = 1 THEN $1 ELSE status END", [None, None, Some("ready")]),
                ("CASE WHEN id = 1 THEN status ELSE $1 END", [Some("ready"), None, None]),
            ],
        ),
    ] {
        for (expression, expected) in cases {
            let result = connection
                .query_params(
                    &format!(
                        "SELECT id, ({expression})::text, pg_typeof($1)::text, \
                         pg_typeof({expression})::text FROM {schema}.rows ORDER BY id"
                    ),
                    std::slice::from_ref(&parameter),
                )
                .await
                .unwrap_or_else(|error| panic!("{expression}, depth {levels}: {error:?}"));
            assert_eq!(
                result.rows,
                expected
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| vec![
                        Value::Int(index as i64 + 1),
                        value.map_or(Value::Null, |value| Value::Text(value.into())),
                        Value::Text(format!("{schema}.state")),
                        Value::Text(format!("{schema}.state")),
                    ])
                    .collect::<Vec<_>>(),
                "{expression}, parameter {parameter:?}, depth {levels}"
            );
        }
    }
}

async fn assert_deep_enum_array_expression_refused(
    connection: &dyn tablepro_core::Connection,
    schema: &str,
    levels: usize,
) {
    for parameter in [Value::Text("paused".into()), Value::Null] {
        let error = connection
            .query_params(
                &format!(
                    "SELECT array_append(ARRAY[status], $1) \
                     FROM {schema}.rows WHERE id = 1"
                ),
                std::slice::from_ref(&parameter),
            )
            .await
            .expect_err("deep enum array parameter must be refused");
        assert!(
            matches!(&error, tablepro_core::DriverError::Unsupported(message)
                if message.contains("resolvable depth")),
            "array_append parameter {parameter:?}, depth {levels}: {error:?}"
        );
    }
}
