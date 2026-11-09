use bookie_core::Value;

#[tokio::test]
#[ignore = "requires docker"]
async fn an_edit_on_a_non_unique_sorting_key_is_refused_and_changes_nothing() {
    let (_container, opts) = super::start_clickhouse().await;
    let conn = super::connect(opts).await;
    conn.execute("CREATE TABLE keyed (a Int32, b Int32, v String) ENGINE = MergeTree ORDER BY (a, b) PRIMARY KEY a")
        .await
        .unwrap();
    conn.execute("INSERT INTO keyed VALUES (1, 1, 'x'), (1, 1, 'y'), (1, 2, 'z')")
        .await
        .unwrap();

    let duplicate = (
        "ALTER TABLE `keyed` UPDATE `v` = ? WHERE `a` = ? AND `b` = ?".to_string(),
        vec![Value::Text("edited".into()), Value::Int(1), Value::Int(1)],
    );
    assert!(conn.execute_in_transaction(&[duplicate]).await.is_err());
    let unique = (
        "ALTER TABLE `keyed` UPDATE `v` = ? WHERE `a` = ? AND `b` = ?".to_string(),
        vec![Value::Text("edited".into()), Value::Int(1), Value::Int(2)],
    );
    conn.execute_in_transaction(&[unique]).await.unwrap();

    let rows = conn.query("SELECT v FROM keyed ORDER BY v").await.unwrap().rows;
    assert_eq!(
        rows,
        vec![
            vec![Value::Text("edited".into())],
            vec![Value::Text("x".into())],
            vec![Value::Text("y".into())]
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_merge_tree_sorting_key_is_not_advertised_as_a_unique_row_identity() {
    let (_container, opts) = super::start_clickhouse().await;
    let conn = super::connect(opts).await;
    conn.execute(
        "CREATE TABLE identity_check (a Int32, b Int32, v String) ENGINE = MergeTree ORDER BY (a, b) PRIMARY KEY a",
    )
    .await
    .unwrap();
    conn.execute("INSERT INTO identity_check VALUES (1, 1, 'x'), (1, 2, 'y')")
        .await
        .unwrap();

    let columns = conn.fetch_columns(None, "identity_check").await.unwrap();
    let key_index = columns.iter().position(|column| column.name == "a").unwrap();
    assert!(
        !columns[key_index].primary_key,
        "a sparse MergeTree primary key is not a unique row identity"
    );
    assert!(matches!(
        bookie_core::sql_dialect::build_keyed_update(
            "clickhouse",
            None,
            "identity_check",
            &columns,
            &[(2, Value::Text("edited".into()))],
            &[Value::Int(1)],
        ),
        Err(bookie_core::sql_dialect::BuildSqlError::NoPrimaryKey)
    ));
}
