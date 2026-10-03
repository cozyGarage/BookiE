use super::parse_input_for_grid_cell;
use tablepro_core::Value;

#[tokio::test]
async fn sqlite_strict_any_grid_edit_preserves_each_rows_runtime_storage_class() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE flexible (id INTEGER PRIMARY KEY, value ANY) STRICT")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO flexible VALUES (1, 1.5), (2, 'text'), (3, 42), (4, NULL)")
        .await
        .unwrap();

    let columns = connection.fetch_columns(None, "flexible").await.unwrap();
    let value_index = columns.iter().position(|column| column.name == "value").unwrap();
    assert_eq!(columns[value_index].data_type, "ANY");
    for (id, input) in [(1, "2.75"), (2, "007"), (3, "43")] {
        let current = connection
            .query_params("SELECT value FROM flexible WHERE id = ?", &[Value::Int(id)])
            .await
            .unwrap();
        let current = current.rows[0][0].clone();
        let edit = parse_input_for_grid_cell(input, Some(&columns[value_index]), "sqlite", Some(&current)).unwrap();
        let update = tablepro_core::sql_dialect::build_keyed_update(
            "sqlite",
            None,
            "flexible",
            &columns,
            &[(value_index, edit)],
            &[Value::Int(id)],
        )
        .unwrap();
        connection.execute_in_transaction(&[update]).await.unwrap();
    }

    let saved = connection
        .query("SELECT id, typeof(value), value FROM flexible ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        saved.rows,
        vec![
            vec![Value::Int(1), Value::Text("real".into()), Value::Float(2.75)],
            vec![Value::Int(2), Value::Text("text".into()), Value::Text("007".into())],
            vec![Value::Int(3), Value::Text("integer".into()), Value::Int(43)],
            vec![Value::Int(4), Value::Text("null".into()), Value::Null],
        ]
    );
}

#[tokio::test]
async fn sqlite_strict_any_null_and_new_cells_use_text_unless_blank() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE flexible (id INTEGER PRIMARY KEY, value ANY) STRICT")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO flexible VALUES (1, NULL), (3, NULL)")
        .await
        .unwrap();

    let columns = connection.fetch_columns(None, "flexible").await.unwrap();
    let id_index = columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = columns.iter().position(|column| column.name == "value").unwrap();
    assert!(columns[id_index].is_auto_increment);
    assert_eq!(columns[value_index].data_type, "ANY");
    let null = Value::Null;
    let edit = parse_input_for_grid_cell("42", Some(&columns[value_index]), "sqlite", Some(&null)).unwrap();
    assert_eq!(edit, Value::Text("42".into()));
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "sqlite",
        None,
        "flexible",
        &columns,
        &[(value_index, edit)],
        &[Value::Int(1)],
    )
    .unwrap();
    connection.execute_in_transaction(&[update]).await.unwrap();

    let blank = parse_input_for_grid_cell("", Some(&columns[value_index]), "sqlite", Some(&null)).unwrap();
    assert_eq!(blank, Value::Null);
    let blank_update = tablepro_core::sql_dialect::build_keyed_update(
        "sqlite",
        None,
        "flexible",
        &columns,
        &[(value_index, blank)],
        &[Value::Int(3)],
    )
    .unwrap();
    connection.execute_in_transaction(&[blank_update]).await.unwrap();

    let mut draft = vec![Value::Null; columns.len()];
    draft[value_index] = parse_input_for_grid_cell("007", Some(&columns[value_index]), "sqlite", Some(&null)).unwrap();
    assert_eq!(draft[value_index], Value::Text("007".into()));
    let (insert_sql, insert_params) =
        tablepro_core::sql_dialect::build_insert_from_draft("sqlite", None, "flexible", &columns, &draft).unwrap();
    connection.execute_params(&insert_sql, &insert_params).await.unwrap();

    let saved = connection
        .query("SELECT id, typeof(value), value FROM flexible ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        saved.rows,
        vec![
            vec![Value::Int(1), Value::Text("text".into()), Value::Text("42".into())],
            vec![Value::Int(3), Value::Text("null".into()), Value::Null],
            vec![Value::Int(4), Value::Text("text".into()), Value::Text("007".into())],
        ]
    );
}
