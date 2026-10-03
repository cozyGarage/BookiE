#[cfg(feature = "duckdb")]
use super::parse_input_for_grid_cell;
#[cfg(feature = "duckdb")]
use tablepro_core::Value;

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn value_contract_duckdb_enum_keyed_edit_preserves_native_type_and_siblings() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_duckdb::DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE mood AS ENUM ('', 'NULL', '東京', 'ready', 'O''Brien')")
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE moods (id INTEGER PRIMARY KEY, status mood)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO moods VALUES (1, ''), (2, 'NULL'), (3, NULL), (4, 'ready'), (5, 'O''Brien')")
        .await
        .unwrap();

    let columns = connection.fetch_columns(None, "moods").await.unwrap();
    let status_index = columns.iter().position(|column| column.name == "status").unwrap();
    assert!(columns[status_index].data_type.starts_with("ENUM"));
    let edit = parse_input_for_grid_cell("ready", Some(&columns[status_index]), "duckdb", None).unwrap();
    assert_eq!(edit, Value::Text("ready".into()));
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "duckdb",
        None,
        "moods",
        &columns,
        &[(status_index, edit)],
        &[Value::Int(2)],
    )
    .unwrap();
    connection.execute_in_transaction(&[update]).await.unwrap();

    assert_eq!(
        parse_input_for_grid_cell("", Some(&columns[status_index]), "duckdb", None).unwrap(),
        Value::Null
    );
    let empty_label = parse_input_for_grid_cell("''", Some(&columns[status_index]), "duckdb", None).unwrap();
    assert_eq!(empty_label, Value::Text(String::new()));
    let empty_update = tablepro_core::sql_dialect::build_keyed_update(
        "duckdb",
        None,
        "moods",
        &columns,
        &[(status_index, empty_label)],
        &[Value::Int(4)],
    )
    .unwrap();
    connection.execute_in_transaction(&[empty_update]).await.unwrap();

    let escaped = parse_input_for_grid_cell("'O''Brien'", Some(&columns[status_index]), "duckdb", None).unwrap();
    assert_eq!(escaped, Value::Text("O'Brien".into()));
    let escaped_update = tablepro_core::sql_dialect::build_keyed_update(
        "duckdb",
        None,
        "moods",
        &columns,
        &[(status_index, escaped)],
        &[Value::Int(2)],
    )
    .unwrap();
    connection.execute_in_transaction(&[escaped_update]).await.unwrap();

    let invalid = parse_input_for_grid_cell("not a label", Some(&columns[status_index]), "duckdb", None).unwrap();
    let invalid_update = tablepro_core::sql_dialect::build_keyed_update(
        "duckdb",
        None,
        "moods",
        &columns,
        &[(status_index, invalid)],
        &[Value::Int(2)],
    )
    .unwrap();
    assert!(connection.execute_in_transaction(&[invalid_update]).await.is_err());

    let saved = connection
        .query("SELECT id, typeof(status), status::VARCHAR FROM moods ORDER BY id")
        .await
        .unwrap();
    let enum_type = match &saved.rows[0][1] {
        Value::Text(type_name) if type_name.starts_with("ENUM") => type_name.clone(),
        other => panic!("expected native ENUM type, got {other:?}"),
    };
    assert_eq!(
        saved.rows,
        vec![
            vec![Value::Int(1), Value::Text(enum_type.clone()), Value::Text("".into())],
            vec![
                Value::Int(2),
                Value::Text(enum_type.clone()),
                Value::Text("O'Brien".into())
            ],
            vec![Value::Int(3), Value::Text(enum_type.clone()), Value::Null],
            vec![Value::Int(4), Value::Text(enum_type.clone()), Value::Text("".into())],
            vec![
                Value::Int(5),
                Value::Text(enum_type.clone()),
                Value::Text("O'Brien".into())
            ],
        ]
    );
}
