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
