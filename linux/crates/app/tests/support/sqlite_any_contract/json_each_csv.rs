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

#[tokio::test]
async fn sqlite_json_constructors_typed_csv_round_trip_preserves_json_text() {
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
                 id INTEGER PRIMARY KEY, object ANY, object_class TEXT, array ANY, array_class TEXT\
             ) STRICT",
        )
        .await
        .unwrap();
    let result = connection
        .query(
            "SELECT json_object('empty', '', 'null', NULL, 'number', 7), \
                    typeof(json_object('empty', '', 'null', NULL, 'number', 7)), \
                    json_array(1, 1.25, '', NULL, json('true'), json('{\"x\":1}')), \
                    typeof(json_array(1, 1.25, '', NULL, json('true'), json('{\"x\":1}'))) ",
        )
        .await
        .unwrap();
    sqlite_result_csv_round_trip(
        connection.as_ref(),
        &result,
        "restored",
        &[None, Some(0), Some(1), Some(2), Some(3)],
    )
    .await;

    let restored = connection
        .query(
            "SELECT typeof(object), object, object_class, typeof(array), array, array_class \
             FROM restored ORDER BY id",
        )
        .await
        .unwrap();
    let row = &result.rows[0];
    assert_eq!(
        restored.rows,
        vec![vec![
            row[1].clone(),
            row[0].clone(),
            row[1].clone(),
            row[3].clone(),
            row[2].clone(),
            row[3].clone(),
        ]]
    );
}

#[tokio::test]
async fn sqlite_jsonb_typed_csv_round_trip_preserves_blob_bytes() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE restored (id INTEGER PRIMARY KEY, value ANY) STRICT")
        .await
        .unwrap();
    let result = connection
        .query(
            "SELECT jsonb('{\"empty\":\"\",\"null\":null,\"nested\":[1,true]}') AS value, \
                    typeof(jsonb('{\"empty\":\"\",\"null\":null,\"nested\":[1,true]}')) AS storage_class, \
                    json(jsonb('{\"empty\":\"\",\"null\":null,\"nested\":[1,true]}')) AS json_text, \
                    hex(jsonb('{\"empty\":\"\",\"null\":null,\"nested\":[1,true]}')) AS native_hex",
        )
        .await
        .unwrap();
    sqlite_result_csv_round_trip(
        connection.as_ref(),
        &result,
        "restored",
        &[None, Some(0), None, None],
    )
    .await;

    let restored = connection
        .query("SELECT typeof(value), hex(value), json(value) FROM restored ORDER BY id")
        .await
        .unwrap();
    let source = &result.rows[0];
    assert_eq!(
        restored.rows,
        vec![vec![
            source[1].clone(),
            source[3].clone(),
            source[2].clone(),
        ]]
    );
}
