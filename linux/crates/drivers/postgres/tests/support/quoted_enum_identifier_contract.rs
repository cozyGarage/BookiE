#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{FilterOp, FilterRule, FilterSet, FilterValue, Value};

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_quoted_enum_identifiers_preserve_metadata_and_typed_writes() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    let schema = "enum audit \"schema\"";
    let type_name = "state \"kind\"";
    let quoted_schema = "\"enum audit \"\"schema\"\"\"";
    let quoted_type = "\"state \"\"kind\"\"\"";
    connection
        .execute(&format!("CREATE SCHEMA {quoted_schema}"))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "CREATE TYPE {quoted_schema}.{quoted_type} AS ENUM ('NULL', '', 'ready', '東京')"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "CREATE TABLE {quoted_schema}.rows (id INT PRIMARY KEY, state {quoted_schema}.{quoted_type}, sibling TEXT NOT NULL)"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "INSERT INTO {quoted_schema}.rows VALUES (1, 'ready', 'keyed'), (2, 'NULL', 'literal-null'), (3, NULL, 'sql-null')"
        ))
        .await
        .unwrap();

    let columns = connection.fetch_columns(Some(schema), "rows").await.unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: schema.into(),
            name: type_name.into(),
        })
    );

    let (update_sql, update_params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some(schema),
        "rows",
        &columns,
        &[(1, Value::Text(String::new()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(
        update_sql.contains(&format!("{quoted_schema}.{quoted_type}")),
        "{update_sql}"
    );
    connection.execute_params(&update_sql, &update_params).await.unwrap();

    let (insert_sql, insert_params) = tablepro_core::sql_dialect::build_insert_from_draft(
        "postgres",
        Some(schema),
        "rows",
        &columns,
        &[Value::Int(4), Value::Text("東京".into()), Value::Text("draft".into())],
    )
    .unwrap();
    assert!(
        insert_sql.contains(&format!("{quoted_schema}.{quoted_type}")),
        "{insert_sql}"
    );
    connection.execute_params(&insert_sql, &insert_params).await.unwrap();

    let (where_sql, filter_params) = tablepro_core::build_filter_where(
        "postgres",
        &columns,
        &FilterSet {
            rules: vec![FilterRule {
                column: "state".into(),
                op: FilterOp::Eq,
                value: Some(FilterValue::Single("NULL".into())),
            }],
            ..Default::default()
        },
    )
    .unwrap()
    .unwrap();
    assert!(
        where_sql.contains(&format!("{quoted_schema}.{quoted_type}")),
        "{where_sql}"
    );
    let filtered = connection
        .query_params(
            &format!("SELECT id, state::text FROM {quoted_schema}.rows WHERE {where_sql}"),
            &filter_params,
        )
        .await
        .unwrap();
    assert_eq!(filtered.rows, vec![vec![Value::Int(2), Value::Text("NULL".into())]]);

    let stored = connection
        .query(&format!(
            "SELECT r.id, r.state::text, n.nspname, t.typname, r.sibling \
             FROM {quoted_schema}.rows r \
             JOIN pg_type t ON t.oid = pg_typeof(r.state)::oid \
             JOIN pg_namespace n ON n.oid = t.typnamespace ORDER BY r.id"
        ))
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text(String::new()),
                Value::Text(schema.into()),
                Value::Text(type_name.into()),
                Value::Text("keyed".into()),
            ],
            vec![
                Value::Int(2),
                Value::Text("NULL".into()),
                Value::Text(schema.into()),
                Value::Text(type_name.into()),
                Value::Text("literal-null".into()),
            ],
            vec![
                Value::Int(3),
                Value::Null,
                Value::Text(schema.into()),
                Value::Text(type_name.into()),
                Value::Text("sql-null".into()),
            ],
            vec![
                Value::Int(4),
                Value::Text("東京".into()),
                Value::Text(schema.into()),
                Value::Text(type_name.into()),
                Value::Text("draft".into()),
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_quoted_enum_identifiers_resist_shadowed_search_path() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    let schema = "enum audit \"schema\"";
    let type_name = "state \"kind\"";
    let quoted_schema = "\"enum audit \"\"schema\"\"\"";
    let quoted_type = "\"state \"\"kind\"\"\"";
    connection
        .execute(&format!("CREATE SCHEMA {quoted_schema}"))
        .await
        .unwrap();
    connection.execute("CREATE SCHEMA enum_quoted_shadow").await.unwrap();
    connection
        .execute(&format!(
            "CREATE TYPE {quoted_schema}.{quoted_type} AS ENUM ('ready', 'target-only', 'NULL')"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "CREATE TYPE enum_quoted_shadow.{quoted_type} AS ENUM ('ready', 'shadow-only')"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "CREATE TABLE {quoted_schema}.rows \
             (id INT PRIMARY KEY, state {quoted_schema}.{quoted_type}, sibling TEXT NOT NULL)"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "CREATE TABLE enum_quoted_shadow.rows \
             (id INT PRIMARY KEY, state enum_quoted_shadow.{quoted_type}, sibling TEXT NOT NULL)"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!(
            "INSERT INTO {quoted_schema}.rows VALUES \
             (1, 'ready', 'target-one'), (2, 'NULL', 'target-two')"
        ))
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO enum_quoted_shadow.rows VALUES \
             (1, 'ready', 'shadow-one'), (2, 'shadow-only', 'shadow-two')",
        )
        .await
        .unwrap();

    let columns = connection.fetch_columns(Some(schema), "rows").await.unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: schema.into(),
            name: type_name.into(),
        })
    );

    let mut transaction = connection.begin().await.unwrap();
    transaction
        .execute("SET LOCAL search_path TO enum_quoted_shadow, public")
        .await
        .unwrap();
    let (update_sql, update_params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some(schema),
        "rows",
        &columns,
        &[(1, Value::Text("target-only".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(
        update_sql.contains(&format!("{quoted_schema}.{quoted_type}")),
        "{update_sql}"
    );
    transaction.execute_params(&update_sql, &update_params).await.unwrap();

    let (where_sql, filter_params) = tablepro_core::build_filter_where(
        "postgres",
        &columns,
        &FilterSet {
            rules: vec![FilterRule {
                column: "state".into(),
                op: FilterOp::Eq,
                value: Some(FilterValue::Single("target-only".into())),
            }],
            ..Default::default()
        },
    )
    .unwrap()
    .unwrap();
    let filtered = transaction
        .query_params(
            &format!("SELECT id, state::text FROM {quoted_schema}.rows WHERE {where_sql}"),
            &filter_params,
        )
        .await
        .unwrap();
    assert_eq!(
        filtered.rows,
        vec![vec![Value::Int(1), Value::Text("target-only".into())]]
    );

    transaction
        .execute("SAVEPOINT reject_shadow_only_enum_label")
        .await
        .unwrap();
    let (invalid_sql, invalid_params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some(schema),
        "rows",
        &columns,
        &[(1, Value::Text("shadow-only".into()))],
        &[Value::Int(2)],
    )
    .unwrap();
    let invalid = transaction
        .execute_params(&invalid_sql, &invalid_params)
        .await
        .expect_err("a label present only in the shadow enum must be refused");
    assert!(
        matches!(&invalid, tablepro_core::DriverError::Query { sqlstate: Some(code), .. } if code == "22P02"),
        "expected target enum invalid-label SQLSTATE 22P02, got {invalid:?}"
    );
    transaction
        .execute("ROLLBACK TO SAVEPOINT reject_shadow_only_enum_label")
        .await
        .unwrap();

    let target = transaction
        .query(&format!(
            "SELECT id, state::text, pg_typeof(state)::text, sibling \
             FROM {quoted_schema}.rows ORDER BY id"
        ))
        .await
        .unwrap();
    assert_eq!(
        target.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("target-only".into()),
                Value::Text(format!("{quoted_schema}.{quoted_type}")),
                Value::Text("target-one".into()),
            ],
            vec![
                Value::Int(2),
                Value::Text("NULL".into()),
                Value::Text(format!("{quoted_schema}.{quoted_type}")),
                Value::Text("target-two".into()),
            ],
        ]
    );
    let shadow = transaction
        .query("SELECT id, state::text, pg_typeof(state)::text, sibling FROM enum_quoted_shadow.rows ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        shadow.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("ready".into()),
                Value::Text(quoted_type.into()),
                Value::Text("shadow-one".into()),
            ],
            vec![
                Value::Int(2),
                Value::Text("shadow-only".into()),
                Value::Text(quoted_type.into()),
                Value::Text("shadow-two".into()),
            ],
        ]
    );
    transaction.rollback().await.unwrap();
}
