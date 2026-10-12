use super::super::super::paste::{parse_clipboard_grid, paste_plan};
use super::super::parse_input_for_grid_cell;
use tablepro_core::{ConnectOptions, DatabaseDriver, Value};

#[tokio::test]
async fn pasted_multiline_text_is_stored_exactly_without_changing_sibling_cells_or_rows() {
    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let control = crate::services::operation_control::bounded(0);
    connection
        .execute_controlled(
            "CREATE TABLE multiline_edit (id INTEGER PRIMARY KEY, body TEXT NOT NULL, sibling TEXT NOT NULL)",
            &control,
        )
        .await
        .unwrap();
    connection
        .execute_controlled(
            "INSERT INTO multiline_edit VALUES (1, 'before one', 'keep one'), (2, 'before two', 'keep two')",
            &control,
        )
        .await
        .unwrap();
    let columns = connection
        .fetch_columns_controlled(None, "multiline_edit", &control)
        .await
        .unwrap();
    let body = columns.iter().position(|column| column.name == "body").unwrap();

    for (row, clipboard, expected) in [
        (0, "\"first\nsecond\"", "first\nsecond"),
        (1, "\"first\r\nsecond\"", "first\r\nsecond"),
    ] {
        let grid = parse_clipboard_grid(clipboard);
        let cell = paste_plan(row, body, &grid, 2, columns.len()).unwrap().remove(0);
        let edit = parse_input_for_grid_cell(&cell.text, Some(&columns[body]), "sqlite", None).unwrap();
        let update = tablepro_core::sql_dialect::build_keyed_update(
            "sqlite",
            None,
            "multiline_edit",
            &columns,
            &[(body, edit)],
            &[Value::Int(i64::from(row + 1))],
        )
        .unwrap();
        assert_eq!(
            connection
                .execute_in_transaction_controlled(&[update], &control)
                .await
                .unwrap(),
            vec![1]
        );
        assert_eq!(cell.text, expected);
    }

    let saved = connection
        .query_controlled("SELECT id, body, sibling FROM multiline_edit ORDER BY id", &control)
        .await
        .unwrap();
    assert_eq!(
        saved.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("first\nsecond".into()),
                Value::Text("keep one".into())
            ],
            vec![
                Value::Int(2),
                Value::Text("first\r\nsecond".into()),
                Value::Text("keep two".into())
            ],
        ]
    );
}
