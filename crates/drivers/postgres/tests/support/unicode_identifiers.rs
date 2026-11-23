use super::*;

#[tokio::test]
#[ignore = "requires docker"]
async fn normalization_distinct_identifiers_remain_distinct_in_catalog_and_reads() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    let composed = "caf\u{e9}";
    let decomposed = "cafe\u{301}";
    for (name, value) in [(composed, 1), (decomposed, 2)] {
        let quoted = bookie_core::sql_dialect::quote_ident("postgres", name);
        connection
            .execute(&format!("CREATE TABLE {quoted} (\"{name}\" integer)"))
            .await
            .unwrap();
        connection
            .execute(&format!("INSERT INTO {quoted} VALUES ({value})"))
            .await
            .unwrap();
    }
    let tables = connection.list_tables().await.unwrap();
    for (name, value) in [(composed, 1), (decomposed, 2)] {
        assert!(tables.iter().any(|table| table.name == name));
        let columns = connection.fetch_columns(Some("public"), name).await.unwrap();
        assert_eq!(columns[0].name, name);
        let quoted = bookie_core::sql_dialect::quote_ident("postgres", name);
        let result = connection
            .query(&format!("SELECT {quoted} FROM {quoted}"))
            .await
            .unwrap();
        assert_eq!(result.rows[0][0], Value::Int(value));
        let export = bookie_core::sql_literal::build_insert_literal(
            "postgres",
            Some("public"),
            name,
            &columns,
            &[Value::Int(value + 10)],
        )
        .unwrap();
        connection.execute(&export).await.unwrap();
        let result = connection
            .query(&format!("SELECT {quoted} FROM {quoted} ORDER BY {quoted}"))
            .await
            .unwrap();
        assert_eq!(result.rows[1][0], Value::Int(value + 10));
    }
}
