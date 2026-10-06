use super::parse_input_for_grid_cell;
use tablepro_core::Value;

#[tokio::test]
async fn sqlite_numeric_affinity_grid_edit_preserves_text_and_sibling_values() {
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
            "CREATE TABLE flexible (\
                 id INTEGER PRIMARY KEY, integer_amount INTEGER, \
                 real_amount REAL, numeric_amount NUMERIC\
             )",
        )
        .await
        .unwrap();
    connection
        .execute("INSERT INTO flexible VALUES (1, 1, 1.5, 1), (2, 2, 2.5, 2)")
        .await
        .unwrap();
    let columns = connection.fetch_columns(None, "flexible").await.unwrap();
    let before = connection.query("SELECT * FROM flexible ORDER BY id").await.unwrap();

    for (name, input) in [
        ("integer_amount", "9223372036854775808"),
        ("real_amount", "1e999"),
        ("numeric_amount", "0.123456789012345678901234567890123"),
    ] {
        let column = columns.iter().find(|column| column.name == name).unwrap();
        assert!(
            parse_input_for_grid_cell(input, Some(column), "sqlite", Some(&Value::Int(1))).is_err(),
            "SQLite grid must reject numeric-looking input that affinity could round: {name}"
        );
    }

    for name in ["integer_amount", "real_amount", "numeric_amount"] {
        let index = columns.iter().position(|column| column.name == name).unwrap();
        let current = &before.rows[0][index];
        let edit = parse_input_for_grid_cell("not numeric", Some(&columns[index]), "sqlite", Some(current))
            .unwrap_or_else(|error| panic!("SQLite affinity column {name} must accept TEXT: {error}"));
        assert_eq!(edit, Value::Text("not numeric".into()), "column {name}");
        for driver in ["mysql", "mssql"] {
            assert!(
                parse_input_for_grid_cell("not numeric", Some(&columns[index]), driver, Some(current)).is_err(),
                "the SQLite affinity fallback must not relax {driver} column {name}"
            );
        }
        let update = tablepro_core::sql_dialect::build_keyed_update(
            "sqlite",
            None,
            "flexible",
            &columns,
            &[(index, edit)],
            &[Value::Int(1)],
        )
        .unwrap();
        connection.execute_in_transaction(&[update]).await.unwrap();
    }

    let after = connection
        .query(
            "SELECT id, typeof(integer_amount), integer_amount, \
                    typeof(real_amount), real_amount, \
                    typeof(numeric_amount), numeric_amount \
             FROM flexible ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        after.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("text".into()),
                Value::Text("not numeric".into()),
                Value::Text("text".into()),
                Value::Text("not numeric".into()),
                Value::Text("text".into()),
                Value::Text("not numeric".into()),
            ],
            vec![
                Value::Int(2),
                Value::Text("integer".into()),
                Value::Int(2),
                Value::Text("real".into()),
                Value::Float(2.5),
                Value::Text("integer".into()),
                Value::Int(2),
            ],
        ]
    );
}
