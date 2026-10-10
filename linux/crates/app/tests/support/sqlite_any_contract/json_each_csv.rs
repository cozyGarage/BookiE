#[tokio::test]
async fn sqlite_json_each_typed_csv_round_trip_preserves_runtime_storage_classes() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE restored (\
                 id INTEGER PRIMARY KEY, key ANY, value ANY, json_type TEXT, \
                 value_class TEXT, atom ANY, atom_class TEXT\
             ) STRICT",
        )
        .await
        .unwrap();
    let result = connection
        .query(
            r#"SELECT key, value, type, typeof(value), atom, typeof(atom)
               FROM json_each('[1,1.25,"text","",null,true,false,{"x":1},[2]]')
               ORDER BY key"#,
        )
        .await
        .unwrap();
    sqlite_result_csv_round_trip(
        connection.as_ref(),
        &result,
        "restored",
        &[None, Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)],
    )
    .await;

    let restored = connection
        .query(
            "SELECT typeof(key), key, typeof(value), value, json_type, value_class, \
                    typeof(atom), atom, atom_class FROM restored ORDER BY id",
        )
        .await
        .unwrap();
    let expected = result
        .rows
        .iter()
        .map(|row| {
            vec![
                Value::Text("integer".into()),
                row[0].clone(),
                row[3].clone(),
                row[1].clone(),
                row[2].clone(),
                row[3].clone(),
                row[5].clone(),
                row[4].clone(),
                row[5].clone(),
            ]
        })
        .collect::<Vec<_>>();
    assert_eq!(restored.rows, expected);
}

#[tokio::test]
async fn sqlite_json_each_object_typed_csv_round_trip_preserves_text_keys() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE restored (\
                 id INTEGER PRIMARY KEY, key ANY, value ANY, json_type TEXT, \
                 key_class TEXT, value_class TEXT, atom ANY, atom_class TEXT\
             ) STRICT",
        )
        .await
        .unwrap();
    let result = connection
        .query(
            r#"SELECT key, value, type, typeof(key), typeof(value), atom, typeof(atom)
               FROM json_each('{"":null,"01":true,"NULL":"","雪":{"x":1}}')
               ORDER BY key"#,
        )
        .await
        .unwrap();
    sqlite_result_csv_round_trip(
        connection.as_ref(),
        &result,
        "restored",
        &[None, Some(0), Some(1), Some(2), Some(3), Some(4), Some(5), Some(6)],
    )
    .await;

    let restored = connection
        .query(
            "SELECT typeof(key), key, typeof(value), value, json_type, key_class, \
                    value_class, typeof(atom), atom, atom_class FROM restored ORDER BY id",
        )
        .await
        .unwrap();
    let expected = result
        .rows
        .iter()
        .map(|row| {
            vec![
                row[3].clone(),
                row[0].clone(),
                row[4].clone(),
                row[1].clone(),
                row[2].clone(),
                row[3].clone(),
                row[4].clone(),
                row[6].clone(),
                row[5].clone(),
                row[6].clone(),
            ]
        })
        .collect::<Vec<_>>();
    assert_eq!(restored.rows, expected);
}
