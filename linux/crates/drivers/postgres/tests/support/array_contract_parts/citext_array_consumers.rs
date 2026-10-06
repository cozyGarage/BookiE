#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_citext_array_is_visible_undecodable() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection.execute("CREATE EXTENSION citext").await.unwrap();
    connection
        .execute("CREATE TABLE citext_array_source (id int, labels citext[])")
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO citext_array_source VALUES
               (1, ARRAY['Foo','foo','NULL','',NULL]::citext[]),
               (2, NULL), (3, ARRAY[]::citext[])",
        )
        .await
        .unwrap();

    let result = connection
        .query(
            "SELECT id, labels, labels::text, pg_typeof(labels)::text,
                    encode(array_send(labels), 'hex')
             FROM citext_array_source ORDER BY id",
        )
        .await
        .unwrap();
    assert!(matches!(&result.rows[0][1], Value::Undecodable(name) if name == "CITEXT[]"));
    assert_eq!(result.rows[0][2], Value::Text(r#"{Foo,foo,"NULL","",NULL}"#.into()));
    assert_eq!(result.rows[0][3], Value::Text("citext[]".into()));
    assert!(matches!(&result.rows[0][4], Value::Text(wire) if !wire.is_empty()));
    assert_eq!(result.rows[1][1], Value::Null);
    assert!(matches!(&result.rows[2][1], Value::Undecodable(name) if name == "CITEXT[]"));

    let case = connection
        .query("SELECT labels[1]::text, labels[2]::text, labels[1] = labels[2] FROM citext_array_source WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(case.rows[0][0], Value::Text("Foo".into()));
    assert_eq!(case.rows[0][1], Value::Text("foo".into()));
    assert_eq!(case.rows[0][2], Value::Bool(true));

    let value = &result.rows[0][1];
    assert!(tablepro_core::sql_literal::render_sql_literal("postgres", value).is_err());
    assert!(connection.query_params("SELECT $1", std::slice::from_ref(value)).await.is_err());
    let csv = tablepro_core::export::render_csv(
        &result.columns,
        &result.rows,
        &Default::default(),
    );
    assert!(csv.contains("<undecodable CITEXT[]>"));
}
