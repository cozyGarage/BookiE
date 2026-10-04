#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{FilterOp, FilterRule, FilterSet, FilterValue, Value};

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_257_levels_preserve_schema_aware_values() {
    let (_container, options) = start_pg().await;
    let setup = connect(options.clone()).await;
    let schema = "value_contract_257_domains";
    let shadow = "value_contract_257_shadow";
    setup.execute(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    setup
        .execute(&format!("CREATE TYPE {schema}.status AS ENUM ('ready', 'paused')"))
        .await
        .unwrap();
    let mut base = "status".to_owned();
    for level in 1..=257 {
        let domain = format!("status_domain_{level}");
        setup
            .execute(&format!("CREATE DOMAIN {schema}.{domain} AS {schema}.{base}"))
            .await
            .unwrap();
        base = domain;
    }
    setup
        .execute(&format!(
            "CREATE TABLE {schema}.rows (id integer PRIMARY KEY, status {schema}.{base})"
        ))
        .await
        .unwrap();
    setup
        .execute(&format!("INSERT INTO {schema}.rows VALUES (1, 'ready'), (2, NULL)"))
        .await
        .unwrap();
    setup.execute(&format!("CREATE SCHEMA {shadow}")).await.unwrap();
    setup
        .execute(&format!("CREATE TYPE {shadow}.status AS ENUM ('shadow')"))
        .await
        .unwrap();
    setup
        .execute(&format!("ALTER ROLE postgres SET search_path TO {shadow}"))
        .await
        .unwrap();
    drop(setup);

    let connection = connect(options).await;
    assert_eq!(
        connection.query("SELECT current_schema()::text").await.unwrap().rows,
        vec![vec![Value::Text(shadow.into())]]
    );
    let columns = connection.fetch_columns(Some(schema), "rows").await.unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: schema.into(),
            name: "status".into()
        })
    );

    let (update, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some(schema),
        "rows",
        &columns,
        &[(1, Value::Text("paused".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(update.contains(&format!("\"{schema}\".\"status\"")), "{update}");
    connection.execute_params(&update, &params).await.unwrap();

    let raw = connection
        .execute_params(
            &format!("UPDATE {schema}.rows SET status = $1 WHERE id = 1"),
            &[Value::Text("ready".into())],
        )
        .await
        .expect_err("deep raw enum inference remains an explicit refusal");
    assert!(
        matches!(&raw, tablepro_core::DriverError::Unsupported(message) if message.contains("resolvable depth")),
        "{raw:?}"
    );

    let filters = FilterSet {
        rules: vec![FilterRule {
            column: "status".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Single("paused".into())),
        }],
        ..Default::default()
    };
    let (where_sql, filter_params) = tablepro_core::build_filter_where("postgres", &columns, &filters)
        .unwrap()
        .unwrap();
    let filtered = connection
        .query_params(
            &format!(
                "SELECT id, status::text, pg_typeof(status)::text \
                 FROM {schema}.rows WHERE {where_sql} ORDER BY id"
            ),
            &filter_params,
        )
        .await
        .unwrap();
    assert_eq!(
        filtered.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("paused".into()),
            Value::Text(format!("{schema}.status_domain_257")),
        ]]
    );

    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute(&format!("SET LOCAL search_path TO {shadow}, public"))
        .await
        .unwrap();
    let (transaction_update, transaction_params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some(schema),
        "rows",
        &columns,
        &[(1, Value::Text("ready".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    transaction
        .execute_params(&transaction_update, &transaction_params)
        .await
        .unwrap();
    let stored = transaction
        .query(&format!(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM {schema}.rows ORDER BY id"
        ))
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("ready".into()),
                Value::Text(format!("{schema}.status_domain_257")),
            ],
            vec![
                Value::Int(2),
                Value::Null,
                Value::Text(format!("{schema}.status_domain_257")),
            ],
        ]
    );
    transaction.rollback().await.unwrap();
    let after_rollback = connection
        .query(&format!(
            "SELECT id, status::text, pg_typeof(status)::text \
             FROM {schema}.rows ORDER BY id"
        ))
        .await
        .unwrap();
    assert_eq!(
        after_rollback.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("paused".into()),
                Value::Text(format!("{schema}.status_domain_257")),
            ],
            vec![
                Value::Int(2),
                Value::Null,
                Value::Text(format!("{schema}.status_domain_257")),
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_enum_258_levels_preserve_schema_aware_values() {
    let (_container, options) = start_pg().await;
    let setup = connect(options.clone()).await;
    let schema = "value_contract_258_domains";
    let shadow = "value_contract_258_shadow";
    setup.execute(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    setup
        .execute(&format!("CREATE TYPE {schema}.status AS ENUM ('ready', 'paused')"))
        .await
        .unwrap();

    let mut base = "status".to_owned();
    for level in 1..=258 {
        let domain = format!("status_domain_{level}");
        setup
            .execute(&format!("CREATE DOMAIN {schema}.{domain} AS {schema}.{base}"))
            .await
            .unwrap();
        base = domain;
    }
    setup
        .execute(&format!(
            "CREATE TABLE {schema}.rows (id integer PRIMARY KEY, status {schema}.{base})"
        ))
        .await
        .unwrap();
    setup
        .execute(&format!("INSERT INTO {schema}.rows VALUES (1, 'ready'), (2, NULL)"))
        .await
        .unwrap();
    setup.execute(&format!("CREATE SCHEMA {shadow}")).await.unwrap();
    setup
        .execute(&format!("CREATE TYPE {shadow}.status AS ENUM ('shadow')"))
        .await
        .unwrap();
    setup
        .execute(&format!("ALTER ROLE postgres SET search_path TO {shadow}"))
        .await
        .unwrap();
    drop(setup);

    let connection = connect(options).await;
    assert_eq!(
        connection.query("SELECT current_schema()::text").await.unwrap().rows,
        vec![vec![Value::Text(shadow.into())]]
    );
    let columns = connection.fetch_columns(Some(schema), "rows").await.unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: schema.into(),
            name: "status".into(),
        })
    );

    let (update, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some(schema),
        "rows",
        &columns,
        &[(1, Value::Text("paused".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(update.contains(&format!("\"{schema}\".\"status\"")), "{update}");
    connection.execute_params(&update, &params).await.unwrap();

    let filters = FilterSet {
        rules: vec![FilterRule {
            column: "status".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Single("paused".into())),
        }],
        ..Default::default()
    };
    let (where_sql, filter_params) = tablepro_core::build_filter_where("postgres", &columns, &filters)
        .unwrap()
        .unwrap();
    let rows = connection
        .query_params(
            &format!(
                "SELECT id, status::text, pg_typeof(status)::text \
                 FROM {schema}.rows WHERE {where_sql} ORDER BY id"
            ),
            &filter_params,
        )
        .await
        .unwrap();
    assert_eq!(
        rows.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("paused".into()),
            Value::Text(format!("{schema}.status_domain_258")),
        ]]
    );
    assert_eq!(
        connection
            .query(&format!(
                "SELECT id, status IS NULL, pg_typeof(status)::text \
                 FROM {schema}.rows ORDER BY id"
            ))
            .await
            .unwrap()
            .rows,
        vec![
            vec![
                Value::Int(1),
                Value::Bool(false),
                Value::Text(format!("{schema}.status_domain_258")),
            ],
            vec![
                Value::Int(2),
                Value::Bool(true),
                Value::Text(format!("{schema}.status_domain_258")),
            ],
        ]
    );
}
