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
        .execute("INSERT INTO flexible VALUES (1, 1.5), (2, 'text'), (3, 42), (4, NULL), (5, 'clear me')")
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

    let current = connection
        .query_params("SELECT value FROM flexible WHERE id = ?", &[Value::Int(5)])
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let clear = parse_input_for_grid_cell("", Some(&columns[value_index]), "sqlite", Some(&current)).unwrap();
    let clear_update = tablepro_core::sql_dialect::build_keyed_update(
        "sqlite",
        None,
        "flexible",
        &columns,
        &[(value_index, clear)],
        &[Value::Int(5)],
    )
    .unwrap();
    connection.execute_in_transaction(&[clear_update]).await.unwrap();

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
            vec![Value::Int(5), Value::Text("text".into()), Value::Text(String::new())],
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

#[tokio::test]
async fn sqlite_strict_any_blob_values_are_read_only_and_keep_exact_bytes() {
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
        .execute("INSERT INTO flexible VALUES (1, X'00FF80')")
        .await
        .unwrap();

    let columns = connection.fetch_columns(None, "flexible").await.unwrap();
    let value_index = columns.iter().position(|column| column.name == "value").unwrap();
    let current = connection
        .query("SELECT value FROM flexible WHERE id = 1")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    assert_eq!(current, Value::Bytes(vec![0x00, 0xff, 0x80]));
    assert!(!crate::ui::grid::cell_allows_inline_edit(
        &columns[value_index],
        &current
    ));

    let saved = connection
        .query("SELECT id, typeof(value), value FROM flexible")
        .await
        .unwrap();
    assert_eq!(
        saved.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("blob".into()),
            Value::Bytes(vec![0x00, 0xff, 0x80]),
        ]]
    );
}

#[tokio::test]
async fn sqlite_strict_any_csv_round_trip_preserves_runtime_storage_classes() {
    use tablepro_core::{
        ConnectOptions, DatabaseDriver,
        export::{CsvOptions, render_csv, unique_csv_null_marker},
        import::{CsvImportOptions, ImportTarget, build_insert_plan, read_csv},
    };

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE source (id INTEGER PRIMARY KEY, value ANY) STRICT")
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE restored (id INTEGER PRIMARY KEY, value ANY) STRICT")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO source VALUES (1, 42), (2, 1.5), (3, '42'), (4, ''), (5, NULL), (6, X'00FF80'), (7, X''), (8, 'bookie:sqlite-any:v1:integer:9'), (9, '=SUM(1)')")
        .await
        .unwrap();

    let source_columns = connection.fetch_columns(None, "source").await.unwrap();
    let value_column = source_columns.iter().find(|column| column.name == "value").unwrap();
    let source_rows = connection
        .query("SELECT value FROM source ORDER BY id")
        .await
        .unwrap()
        .rows;
    let null_marker = unique_csv_null_marker(&source_rows);
    let csv = render_csv(
        std::slice::from_ref(value_column),
        &source_rows,
        &CsvOptions {
            null_to_empty: false,
            null_marker: Some(null_marker.clone()),
            ..CsvOptions::default()
        },
    );

    let sheet = read_csv(
        csv.as_bytes(),
        &CsvImportOptions {
            has_header: true,
            null_marker: null_marker.clone(),
            ..CsvImportOptions::default()
        },
        None,
    )
    .unwrap();
    let target_columns = connection.fetch_columns(None, "restored").await.unwrap();
    let plan = build_insert_plan(
        &ImportTarget {
            driver_id: "sqlite",
            schema: None,
            table: "restored",
            columns: &target_columns,
            mapping: &[None, Some(0)],
        },
        &sheet,
        &CsvImportOptions {
            has_header: true,
            null_marker: null_marker.clone(),
            ..CsvImportOptions::default()
        },
    )
    .unwrap();
    for row in &plan.rows {
        connection.execute_params(&plan.statement, row).await.unwrap();
    }

    let restored = connection
        .query("SELECT typeof(value), value FROM restored ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![Value::Text("integer".into()), Value::Int(42)],
            vec![Value::Text("real".into()), Value::Float(1.5)],
            vec![Value::Text("text".into()), Value::Text("42".into())],
            vec![Value::Text("text".into()), Value::Text(String::new())],
            vec![Value::Text("null".into()), Value::Null],
            vec![Value::Text("blob".into()), Value::Bytes(vec![0x00, 0xff, 0x80])],
            vec![Value::Text("blob".into()), Value::Bytes(vec![])],
            vec![
                Value::Text("text".into()),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
            ],
            vec![Value::Text("text".into()), Value::Text("=SUM(1)".into())],
        ]
    );
    let source_real = match source_rows[1][0] {
        Value::Float(value) => value.to_bits(),
        ref other => panic!("expected source REAL value, got {other:?}"),
    };
    let restored_real = match restored.rows[1][1] {
        Value::Float(value) => value.to_bits(),
        ref other => panic!("expected restored REAL value, got {other:?}"),
    };
    assert_eq!(restored_real, source_real);
}

#[tokio::test]
async fn sqlite_strict_any_csv_import_refuses_ambiguous_blank_and_bad_tags() {
    use tablepro_core::{
        ConnectOptions, DatabaseDriver,
        import::{CsvImportOptions, ImportTarget, PlanError, build_insert_plan, read_csv},
    };

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
    let columns = connection.fetch_columns(None, "flexible").await.unwrap();

    let import = |bytes: &[u8], null_marker: String| {
        let options = CsvImportOptions {
            has_header: true,
            null_marker,
            ..CsvImportOptions::default()
        };
        let sheet = read_csv(bytes, &options, None).unwrap();
        build_insert_plan(
            &ImportTarget {
                driver_id: "sqlite",
                schema: None,
                table: "flexible",
                columns: &columns,
                mapping: &[None, Some(0)],
            },
            &sheet,
            &options,
        )
    };

    let blank = import(b"value\n\"\"\n", String::new()).unwrap_err();
    assert!(matches!(
        blank,
        PlanError::Rows { first, total: 1 }
            if first[0].reason == tablepro_core::import::CellError::AmbiguousSqliteAnyNullOrEmpty
    ));

    let malformed = import(b"value\nbookie:sqlite-any:v1:blob:0xz1\n", "\\N".into()).unwrap_err();
    assert!(matches!(
        malformed,
        PlanError::Rows { first, total: 1 }
            if first[0].reason == tablepro_core::import::CellError::InvalidSqliteAnyCsvValue
    ));

    let untagged_text = import(b"value\n42\n", "\\N".into()).unwrap();
    assert_eq!(untagged_text.rows, vec![vec![Value::Text("42".into())]]);
    assert_eq!(
        connection.query("SELECT COUNT(*) FROM flexible").await.unwrap().rows,
        vec![vec![Value::Int(0)]]
    );
}
