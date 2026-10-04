use super::parse_input_for_grid_cell;
use tablepro_core::Value;

async fn sqlite_result_csv_round_trip(
    connection: &dyn tablepro_core::Connection,
    result: &tablepro_core::QueryResult,
    target_table: &str,
    mapping: &[Option<usize>],
) {
    use tablepro_core::{
        export::{CsvOptions, ResultExport, ResultFormat, unique_csv_null_marker, write_result_file},
        import::{CsvImportOptions, ImportTarget, build_insert_plan, read_csv},
    };

    let null_marker = unique_csv_null_marker(&result.rows);
    let options = CsvOptions {
        null_to_empty: false,
        null_marker: Some(null_marker.clone()),
        preserve_sqlite_result_types: true,
        ..CsvOptions::default()
    };
    let directory = tempfile::tempdir().unwrap();
    let csv_path = directory.path().join("sqlite-result.csv");
    write_result_file(
        &csv_path,
        result,
        &ResultExport {
            format: ResultFormat::Csv,
            csv: &options,
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let csv = std::fs::read(&csv_path).unwrap();
    let sheet = read_csv(
        &csv,
        &CsvImportOptions {
            has_header: true,
            null_marker: null_marker.clone(),
            ..CsvImportOptions::default()
        },
        None,
    )
    .unwrap();
    let target_columns = connection.fetch_columns(None, target_table).await.unwrap();
    let plan = build_insert_plan(
        &ImportTarget {
            driver_id: "sqlite",
            schema: None,
            table: target_table,
            columns: &target_columns,
            mapping,
        },
        &sheet,
        &CsvImportOptions {
            has_header: true,
            null_marker,
            ..CsvImportOptions::default()
        },
    )
    .unwrap();
    for row in &plan.rows {
        connection.execute_params(&plan.statement, row).await.unwrap();
    }
}

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

    let source_result = connection.query("SELECT value FROM source ORDER BY id").await.unwrap();
    assert_eq!(source_result.columns[0].data_type, "ANY");
    let empty_result = connection.query("SELECT value FROM source WHERE 0").await.unwrap();
    assert_eq!(empty_result.columns[0].data_type, "ANY");
    assert!(empty_result.rows.is_empty());
    assert_eq!(
        render_csv(&empty_result.columns, &empty_result.rows, &CsvOptions::default()),
        "value\n"
    );
    let source_rows = source_result.rows;
    let null_marker = unique_csv_null_marker(&source_rows);
    let csv = render_csv(
        &source_result.columns,
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
async fn sqlite_compound_any_result_exports_csv_text_and_xlsx_cell_kinds() {
    use tablepro_core::{
        ConnectOptions, DatabaseDriver,
        export::{CsvOptions, ResultExport, ResultFormat, render_csv, write_result_file},
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
    connection
        .execute("INSERT INTO flexible VALUES (1, 42), (2, NULL)")
        .await
        .unwrap();
    let result = connection
        .query(
            "SELECT 1 AS position, value AS result, typeof(value) AS storage_class \
             FROM flexible WHERE id = 1 \
             UNION ALL SELECT 2, 'branch', typeof('branch') \
             UNION ALL SELECT 3, value, typeof(value) FROM flexible WHERE id = 2 \
             ORDER BY position",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(
        render_csv(&result.columns, &result.rows, &CsvOptions::default()),
        "position,result,storage_class\n1,42,integer\n2,branch,text\n3,,null\n"
    );

    let directory = tempfile::tempdir().unwrap();
    let workbook_path = directory.path().join("sqlite-compound-any.xlsx");
    write_result_file(
        &workbook_path,
        &result,
        &ResultExport {
            format: ResultFormat::Xlsx,
            csv: &CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(workbook_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    assert!(sheet.contains("<c r=\"B2\"><v>42</v></c>"), "{sheet}");
    assert!(sheet.contains("<c r=\"B3\" t=\"s\">"), "{sheet}");
    assert!(!sheet.contains("r=\"B4\""), "{sheet}");
    assert!(shared_strings.contains("<t>branch</t>"), "{shared_strings}");
}

#[tokio::test]
async fn sqlite_compound_any_csv_round_trip_preserves_runtime_storage_classes() {
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
        .execute("INSERT INTO flexible VALUES (1, 42), (2, NULL), (3, X'00FF')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE restored (\
                 id INTEGER PRIMARY KEY, position INTEGER, result ANY, storage_class TEXT\
             ) STRICT",
        )
        .await
        .unwrap();
    let result = connection
        .query(
            "SELECT 1 AS position, value AS result, typeof(value) AS storage_class \
             FROM flexible WHERE id = 1 \
             UNION ALL SELECT 2, 'branch', typeof('branch') \
             UNION ALL SELECT 3, value, typeof(value) FROM flexible WHERE id = 2 \
             UNION ALL SELECT 4, value, typeof(value) FROM flexible WHERE id = 3 \
             UNION ALL SELECT 5, 'bookie:sqlite-any:v1:integer:9', typeof('bookie:sqlite-any:v1:integer:9') \
             ORDER BY position",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");

    sqlite_result_csv_round_trip(
        connection.as_ref(),
        &result,
        "restored",
        &[None, Some(0), Some(1), Some(2)],
    )
    .await;

    let restored = connection
        .query("SELECT typeof(position), position, typeof(result), result, storage_class FROM restored ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![
                Value::Text("integer".into()),
                Value::Int(1),
                Value::Text("integer".into()),
                Value::Int(42),
                Value::Text("integer".into()),
            ],
            vec![
                Value::Text("integer".into()),
                Value::Int(2),
                Value::Text("text".into()),
                Value::Text("branch".into()),
                Value::Text("text".into()),
            ],
            vec![
                Value::Text("integer".into()),
                Value::Int(3),
                Value::Text("null".into()),
                Value::Null,
                Value::Text("null".into()),
            ],
            vec![
                Value::Text("integer".into()),
                Value::Int(4),
                Value::Text("blob".into()),
                Value::Bytes(vec![0, 255]),
                Value::Text("blob".into()),
            ],
            vec![
                Value::Text("integer".into()),
                Value::Int(5),
                Value::Text("text".into()),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
                Value::Text("text".into()),
            ],
        ]
    );
}

#[tokio::test]
async fn sqlite_case_any_csv_round_trip_preserves_runtime_storage_classes() {
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
        .execute(
            "INSERT INTO flexible VALUES \
             (1, 42), (2, NULL), (3, NULL), (4, X'00FF'), \
             (5, 'bookie:sqlite-any:v1:integer:9')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE restored (\
                 id INTEGER PRIMARY KEY, position INTEGER, result ANY, storage_class TEXT\
             ) STRICT",
        )
        .await
        .unwrap();
    let result = connection
        .query(
            "SELECT id AS position, \
                    CASE id WHEN 2 THEN 'branch' WHEN 3 THEN NULL ELSE value END AS result, \
                    typeof(CASE id WHEN 2 THEN 'branch' WHEN 3 THEN NULL ELSE value END) AS storage_class \
             FROM flexible ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(
        result.rows.iter().map(|row| row[2].clone()).collect::<Vec<_>>(),
        vec![
            Value::Text("integer".into()),
            Value::Text("text".into()),
            Value::Text("null".into()),
            Value::Text("blob".into()),
            Value::Text("text".into()),
        ]
    );
    sqlite_result_csv_round_trip(
        connection.as_ref(),
        &result,
        "restored",
        &[None, Some(0), Some(1), Some(2)],
    )
    .await;

    let restored = connection
        .query("SELECT typeof(result), result FROM restored ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![Value::Text("integer".into()), Value::Int(42)],
            vec![Value::Text("text".into()), Value::Text("branch".into())],
            vec![Value::Text("null".into()), Value::Null],
            vec![Value::Text("blob".into()), Value::Bytes(vec![0, 255])],
            vec![
                Value::Text("text".into()),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
            ],
        ]
    );
}

#[tokio::test]
async fn sqlite_coalesce_any_csv_round_trip_preserves_runtime_storage_classes() {
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
        .execute(
            "INSERT INTO flexible VALUES \
             (1, 42), (2, NULL), (3, 1.5), (4, 'ready'), (5, X'00FF'), \
             (6, 'bookie:sqlite-any:v1:integer:9')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE restored (\
                 id INTEGER PRIMARY KEY, position INTEGER, result ANY, storage_class TEXT\
             ) STRICT",
        )
        .await
        .unwrap();
    let result = connection
        .query(
            "SELECT id AS position, coalesce(value, 'fallback') AS result, \
                    typeof(coalesce(value, 'fallback')) AS storage_class \
             FROM flexible ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(
        result.rows.iter().map(|row| row[2].clone()).collect::<Vec<_>>(),
        vec![
            Value::Text("integer".into()),
            Value::Text("text".into()),
            Value::Text("real".into()),
            Value::Text("text".into()),
            Value::Text("blob".into()),
            Value::Text("text".into()),
        ]
    );
    let directory = tempfile::tempdir().unwrap();
    let workbook_path = directory.path().join("sqlite-coalesce-any.xlsx");
    tablepro_core::export::write_result_file(
        &workbook_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(workbook_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    assert!(sheet.contains("<c r=\"B2\"><v>42</v></c>"), "{sheet}");
    assert!(sheet.contains("<c r=\"B3\" t=\"s\">"), "{sheet}");
    assert!(sheet.contains("<c r=\"B4\"><v>1.5</v></c>"), "{sheet}");
    for row in [5, 6, 7] {
        assert!(sheet.contains(&format!("<c r=\"B{row}\" t=\"s\">")), "{sheet}");
    }
    for text in ["fallback", "ready", "\\x00ff", "bookie:sqlite-any:v1:integer:9"] {
        assert!(shared_strings.contains(&format!("<t>{text}</t>")), "{shared_strings}");
    }
    assert!(!sheet.contains("<f>"), "{sheet}");

    let json_path = directory.path().join("sqlite-coalesce-any.json");
    tablepro_core::export::write_result_file(
        &json_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Json,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(json_path).unwrap()).unwrap();
    assert_eq!(json[0]["result"], 42);
    assert_eq!(json[1]["result"], "fallback");
    assert_eq!(json[2]["result"], 1.5);
    assert_eq!(json[3]["result"], "ready");
    assert_eq!(json[4]["result"], "\\x00ff");
    assert_eq!(json[5]["result"], "bookie:sqlite-any:v1:integer:9");
    assert_eq!(json[0]["storage_class"], "integer");
    assert_eq!(json[1]["storage_class"], "text");
    assert_eq!(json[2]["storage_class"], "real");

    sqlite_result_csv_round_trip(
        connection.as_ref(),
        &result,
        "restored",
        &[None, Some(0), Some(1), Some(2)],
    )
    .await;

    let restored = connection
        .query("SELECT typeof(result), result FROM restored ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![Value::Text("integer".into()), Value::Int(42)],
            vec![Value::Text("text".into()), Value::Text("fallback".into())],
            vec![Value::Text("real".into()), Value::Float(1.5)],
            vec![Value::Text("text".into()), Value::Text("ready".into())],
            vec![Value::Text("blob".into()), Value::Bytes(vec![0, 255])],
            vec![
                Value::Text("text".into()),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
            ],
        ]
    );
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
