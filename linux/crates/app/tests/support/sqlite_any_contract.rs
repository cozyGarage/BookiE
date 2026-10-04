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
             UNION ALL SELECT 2, '42', typeof('42') \
             UNION ALL SELECT 3, value, typeof(value) FROM flexible WHERE id = 2 \
             ORDER BY position",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(
        render_csv(&result.columns, &result.rows, &CsvOptions::default()),
        "position,result,storage_class\n1,42,integer\n2,42,text\n3,,null\n"
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
    if let Some(path) = std::env::var_os("BOOKIEE_XLSX_REIMPORT_ARTIFACT") {
        std::fs::copy(&workbook_path, path).unwrap();
    }
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
    assert!(shared_strings.contains("<t>42</t>"), "{shared_strings}");
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
             (5, 'bookie:sqlite-any:v1:integer:9'), (6, '')",
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
            Value::Text("text".into()),
        ]
    );
    let directory = tempfile::tempdir().unwrap();
    let workbook_path = directory.path().join("sqlite-case-any.xlsx");
    std::fs::write(&workbook_path, b"existing workbook").unwrap();
    let error = tablepro_core::export::write_result_file(
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
    .unwrap_err();
    assert!(
        matches!(
            error,
            tablepro_core::export::ExportError::WorkbookEmptyText { row: 6, column: 2 }
        ),
        "{error:?}"
    );
    assert_eq!(std::fs::read(&workbook_path).unwrap(), b"existing workbook");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);

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
            vec![Value::Text("text".into()), Value::Text(String::new())],
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

#[tokio::test]
async fn sqlite_nullif_any_csv_round_trip_preserves_runtime_storage_classes() {
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
             (1, 42), (2, 1.5), (3, 'keep'), (4, 'drop'), \
             (5, X'00FF'), (6, NULL), (7, 'bookie:sqlite-any:v1:integer:9')",
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
            "SELECT id AS position, NULLIF(value, 'drop') AS result, \
                    typeof(NULLIF(value, 'drop')) AS storage_class \
             FROM flexible ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(
        result.rows.iter().map(|row| row[2].clone()).collect::<Vec<_>>(),
        vec![
            Value::Text("integer".into()),
            Value::Text("real".into()),
            Value::Text("text".into()),
            Value::Text("null".into()),
            Value::Text("blob".into()),
            Value::Text("null".into()),
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
        .query("SELECT typeof(result), result, storage_class FROM restored ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![
                Value::Text("integer".into()),
                Value::Int(42),
                Value::Text("integer".into())
            ],
            vec![
                Value::Text("real".into()),
                Value::Float(1.5),
                Value::Text("real".into())
            ],
            vec![
                Value::Text("text".into()),
                Value::Text("keep".into()),
                Value::Text("text".into())
            ],
            vec![Value::Text("null".into()), Value::Null, Value::Text("null".into())],
            vec![
                Value::Text("blob".into()),
                Value::Bytes(vec![0, 255]),
                Value::Text("blob".into())
            ],
            vec![Value::Text("null".into()), Value::Null, Value::Text("null".into())],
            vec![
                Value::Text("text".into()),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
                Value::Text("text".into()),
            ],
        ]
    );
}

#[tokio::test]
async fn sqlite_max_any_csv_round_trip_preserves_runtime_storage_classes() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE flexible (group_id INTEGER, value ANY) STRICT")
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO flexible VALUES \
             (1, 2), (1, 9), (2, 1.25), (2, 1.5), \
             (3, 'alpha'), (3, 'zebra'), (4, X'0001'), (4, X'00FF'), \
             (5, NULL), (5, NULL), (6, 'alpha'), (6, 'bookie:sqlite-any:v1:integer:9')",
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
            "SELECT group_id AS position, MAX(value) AS result, \
                    typeof(MAX(value)) AS storage_class \
             FROM flexible GROUP BY group_id ORDER BY group_id",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(
        result.rows,
        vec![
            vec![Value::Int(1), Value::Int(9), Value::Text("integer".into())],
            vec![Value::Int(2), Value::Float(1.5), Value::Text("real".into())],
            vec![Value::Int(3), Value::Text("zebra".into()), Value::Text("text".into())],
            vec![Value::Int(4), Value::Bytes(vec![0, 255]), Value::Text("blob".into())],
            vec![Value::Int(5), Value::Null, Value::Text("null".into())],
            vec![
                Value::Int(6),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
                Value::Text("text".into()),
            ],
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
        .query("SELECT typeof(result), result, storage_class FROM restored ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![
                Value::Text("integer".into()),
                Value::Int(9),
                Value::Text("integer".into())
            ],
            vec![
                Value::Text("real".into()),
                Value::Float(1.5),
                Value::Text("real".into())
            ],
            vec![
                Value::Text("text".into()),
                Value::Text("zebra".into()),
                Value::Text("text".into())
            ],
            vec![
                Value::Text("blob".into()),
                Value::Bytes(vec![0, 255]),
                Value::Text("blob".into())
            ],
            vec![Value::Text("null".into()), Value::Null, Value::Text("null".into())],
            vec![
                Value::Text("text".into()),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
                Value::Text("text".into()),
            ],
        ]
    );
}

#[tokio::test]
async fn sqlite_min_any_csv_round_trip_preserves_runtime_storage_classes() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE flexible (group_id INTEGER, value ANY) STRICT")
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO flexible VALUES \
             (1, 2), (1, 9), (2, 1.25), (2, 2.5), \
             (3, 'zebra'), (3, 'alpha'), (4, X'00FF'), (4, X'0001'), \
             (5, NULL), (5, NULL), (6, 9), (6, '1'), \
             (7, 'zebra'), (7, 'bookie:sqlite-any:v1:integer:9')",
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
            "SELECT group_id AS position, MIN(value) AS result, \
                    typeof(MIN(value)) AS storage_class \
             FROM flexible GROUP BY group_id ORDER BY group_id",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(
        result.rows,
        vec![
            vec![Value::Int(1), Value::Int(2), Value::Text("integer".into())],
            vec![Value::Int(2), Value::Float(1.25), Value::Text("real".into())],
            vec![Value::Int(3), Value::Text("alpha".into()), Value::Text("text".into())],
            vec![Value::Int(4), Value::Bytes(vec![0, 1]), Value::Text("blob".into())],
            vec![Value::Int(5), Value::Null, Value::Text("null".into())],
            vec![Value::Int(6), Value::Int(9), Value::Text("integer".into())],
            vec![
                Value::Int(7),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
                Value::Text("text".into()),
            ],
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
        .query("SELECT typeof(result), result, storage_class FROM restored ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![
                Value::Text("integer".into()),
                Value::Int(2),
                Value::Text("integer".into())
            ],
            vec![
                Value::Text("real".into()),
                Value::Float(1.25),
                Value::Text("real".into())
            ],
            vec![
                Value::Text("text".into()),
                Value::Text("alpha".into()),
                Value::Text("text".into())
            ],
            vec![
                Value::Text("blob".into()),
                Value::Bytes(vec![0, 1]),
                Value::Text("blob".into())
            ],
            vec![Value::Text("null".into()), Value::Null, Value::Text("null".into())],
            vec![
                Value::Text("integer".into()),
                Value::Int(9),
                Value::Text("integer".into())
            ],
            vec![
                Value::Text("text".into()),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
                Value::Text("text".into()),
            ],
        ]
    );
}

#[tokio::test]
async fn sqlite_group_concat_any_csv_round_trip_preserves_text_and_null() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE flexible (group_id INTEGER, position INTEGER, value ANY) STRICT")
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO flexible VALUES \
             (1, 1, 2), (1, 2, NULL), (1, 3, 1.5), (1, 4, 'alpha'), \
             (2, 1, 'a|b'), (2, 2, ''), (2, 3, 'tail'), \
             (3, 1, NULL), (3, 2, NULL), \
             (4, 1, 'bookie:sqlite-any:v1:integer:9'), (4, 2, 'end')",
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
            "SELECT group_id AS position, \
                    group_concat(value, '<>' ORDER BY position) AS result, \
                    typeof(group_concat(value, '<>' ORDER BY position)) AS storage_class \
             FROM flexible GROUP BY group_id ORDER BY group_id",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("2<>1.5<>alpha".into()),
                Value::Text("text".into()),
            ],
            vec![
                Value::Int(2),
                Value::Text("a|b<><>tail".into()),
                Value::Text("text".into()),
            ],
            vec![Value::Int(3), Value::Null, Value::Text("null".into())],
            vec![
                Value::Int(4),
                Value::Text("bookie:sqlite-any:v1:integer:9<>end".into()),
                Value::Text("text".into()),
            ],
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
        .query("SELECT typeof(result), result, storage_class FROM restored ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![
                Value::Text("text".into()),
                Value::Text("2<>1.5<>alpha".into()),
                Value::Text("text".into()),
            ],
            vec![
                Value::Text("text".into()),
                Value::Text("a|b<><>tail".into()),
                Value::Text("text".into()),
            ],
            vec![Value::Text("null".into()), Value::Null, Value::Text("null".into())],
            vec![
                Value::Text("text".into()),
                Value::Text("bookie:sqlite-any:v1:integer:9<>end".into()),
                Value::Text("text".into()),
            ],
        ]
    );
}

#[tokio::test]
async fn sqlite_sum_any_csv_round_trip_preserves_runtime_storage_classes() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE flexible (group_id INTEGER, value ANY) STRICT")
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO flexible VALUES \
             (1, 7), (1, 9), (2, 1.25), (2, 2.5), \
             (3, '12'), (3, '3'), (4, '1.5'), \
             (5, 'alpha'), (5, X'00FF'), (6, NULL), (6, NULL), \
             (7, 1), (7, 2.5)",
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
            "SELECT group_id AS position, SUM(value) AS result, \
                    typeof(SUM(value)) AS storage_class \
             FROM flexible GROUP BY group_id ORDER BY group_id",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(
        result.rows,
        vec![
            vec![Value::Int(1), Value::Int(16), Value::Text("integer".into())],
            vec![Value::Int(2), Value::Float(3.75), Value::Text("real".into())],
            vec![Value::Int(3), Value::Int(15), Value::Text("integer".into())],
            vec![Value::Int(4), Value::Float(1.5), Value::Text("real".into())],
            vec![Value::Int(5), Value::Float(0.0), Value::Text("real".into())],
            vec![Value::Int(6), Value::Null, Value::Text("null".into())],
            vec![Value::Int(7), Value::Float(3.5), Value::Text("real".into())],
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
        .query("SELECT typeof(result), result, storage_class FROM restored ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        result
            .rows
            .iter()
            .map(|row| vec![
                match &row[1] {
                    Value::Int(_) => Value::Text("integer".into()),
                    Value::Float(_) => Value::Text("real".into()),
                    Value::Null => Value::Text("null".into()),
                    other => panic!("unexpected SUM result: {other:?}"),
                },
                row[1].clone(),
                row[2].clone(),
            ])
            .collect::<Vec<_>>()
    );

    let overflow = connection
        .query(
            "SELECT SUM(value) FROM (\
                 SELECT 9223372036854775807 AS value UNION ALL SELECT 1 AS value\
             )",
        )
        .await
        .unwrap_err();
    assert!(overflow.to_string().contains("integer overflow"), "{overflow}");
}

#[tokio::test]
async fn sqlite_avg_any_csv_round_trip_preserves_runtime_storage_classes() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE flexible (group_id INTEGER, value ANY) STRICT")
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO flexible VALUES \
             (1, 7), (1, 9), (2, 1.25), (2, 2.5), \
             (3, '12'), (3, '3'), (4, '1.5'), \
             (5, 'alpha'), (5, X'00FF'), (6, NULL), (6, NULL), \
             (7, 1), (7, 2.5), \
             (8, 9223372036854775807), (8, 1)",
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
            "SELECT group_id AS position, AVG(value) AS result, \
                    typeof(AVG(value)) AS storage_class \
             FROM flexible GROUP BY group_id ORDER BY group_id",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");

    let expected = [
        (1, Value::Float(8.0), "real"),
        (2, Value::Float(1.875), "real"),
        (3, Value::Float(7.5), "real"),
        (4, Value::Float(1.5), "real"),
        (5, Value::Float(0.0), "real"),
        (6, Value::Null, "null"),
        (7, Value::Float(1.75), "real"),
        (8, Value::Float((i64::MAX as f64 + 1.0) / 2.0), "real"),
    ];
    let expected_result_rows = expected
        .iter()
        .map(|(group_id, value, storage_class)| {
            vec![
                Value::Int(*group_id),
                value.clone(),
                Value::Text((*storage_class).into()),
            ]
        })
        .collect::<Vec<_>>();
    assert_eq!(result.rows, expected_result_rows);

    sqlite_result_csv_round_trip(
        connection.as_ref(),
        &result,
        "restored",
        &[None, Some(0), Some(1), Some(2)],
    )
    .await;

    let restored = connection
        .query("SELECT typeof(result), result, storage_class FROM restored ORDER BY id")
        .await
        .unwrap();
    let expected_restored_rows = expected
        .iter()
        .map(|(_, value, storage_class)| {
            vec![
                Value::Text((*storage_class).into()),
                value.clone(),
                Value::Text((*storage_class).into()),
            ]
        })
        .collect::<Vec<_>>();
    assert_eq!(restored.rows, expected_restored_rows);
}

#[tokio::test]
async fn sqlite_any_arithmetic_csv_round_trip_preserves_runtime_storage_classes() {
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
             (1, 10), (2, 1.5), (3, '42'), (4, '1.5'), \
             (5, 'alpha'), (6, X'00FF'), (7, NULL), \
             (8, 9223372036854775807), (9, 3), (10, 0)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE restored (\
                 id INTEGER PRIMARY KEY, position INTEGER, added ANY, addition_class TEXT, \
                 halved ANY, half_class TEXT, divided_by_zero ANY, zero_class TEXT\
             ) STRICT",
        )
        .await
        .unwrap();

    let result = connection
        .query(
            "SELECT id AS position, value + 1 AS added, typeof(value + 1) AS addition_class, \
                    value / 2 AS halved, typeof(value / 2) AS half_class, \
                    value / 0 AS divided_by_zero, typeof(value / 0) AS zero_class \
             FROM flexible ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(result.columns[3].data_type, "NULL");
    assert_eq!(result.columns[5].data_type, "NULL");
    let expected_row = |id: i64, added: Value, addition_class: &str, halved: Value, half_class: &str| {
        vec![
            Value::Int(id),
            added,
            Value::Text(addition_class.into()),
            halved,
            Value::Text(half_class.into()),
            Value::Null,
            Value::Text("null".into()),
        ]
    };
    assert_eq!(
        result.rows,
        vec![
            expected_row(1, Value::Int(11), "integer", Value::Int(5), "integer"),
            expected_row(2, Value::Float(2.5), "real", Value::Float(0.75), "real"),
            expected_row(3, Value::Int(43), "integer", Value::Int(21), "integer"),
            expected_row(4, Value::Float(2.5), "real", Value::Float(0.75), "real"),
            expected_row(5, Value::Int(1), "integer", Value::Int(0), "integer"),
            expected_row(6, Value::Int(1), "integer", Value::Int(0), "integer"),
            expected_row(7, Value::Null, "null", Value::Null, "null"),
            expected_row(
                8,
                Value::Float(i64::MAX as f64 + 1.0),
                "real",
                Value::Int(4_611_686_018_427_387_903),
                "integer",
            ),
            expected_row(9, Value::Int(4), "integer", Value::Int(1), "integer"),
            expected_row(10, Value::Int(1), "integer", Value::Int(0), "integer"),
        ]
    );

    sqlite_result_csv_round_trip(
        connection.as_ref(),
        &result,
        "restored",
        &[None, Some(0), Some(1), Some(2), Some(3), Some(4), Some(5), Some(6)],
    )
    .await;

    let restored = connection
        .query(
            "SELECT typeof(added), added, addition_class, typeof(halved), halved, half_class, \
                    typeof(divided_by_zero), divided_by_zero, zero_class \
             FROM restored ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        result
            .rows
            .iter()
            .map(|row| vec![
                row[2].clone(),
                row[1].clone(),
                row[2].clone(),
                row[4].clone(),
                row[3].clone(),
                row[4].clone(),
                row[6].clone(),
                row[5].clone(),
                row[6].clone(),
            ])
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn sqlite_json_extract_any_csv_round_trip_preserves_runtime_storage_classes() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE flexible (id INTEGER PRIMARY KEY, document ANY) STRICT")
        .await
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO flexible VALUES
                 (1, '{"v":9223372036854775807}'),
                 (2, '{"v":1.25}'),
                 (3, '{"v":"42"}'),
                 (4, '{"v":true}'),
                 (5, '{"v":null}'),
                 (6, '{"v":{"x":1}}'),
                 (7, '{"v":[1,2]}'),
                 (8, '{"v":"bookie:sqlite-any:v1:integer:9"}'),
                 (9, '{}')"#,
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE restored (\
                 id INTEGER PRIMARY KEY, position INTEGER, result ANY, \
                 storage_class TEXT, json_kind TEXT\
             ) STRICT",
        )
        .await
        .unwrap();

    let result = connection
        .query(
            "SELECT id AS position, json_extract(document, '$.v') AS result, \
                    typeof(json_extract(document, '$.v')) AS storage_class, \
                    json_type(document, '$.v') AS json_kind \
             FROM flexible ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(result.columns[1].data_type, "NULL");
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Int(i64::MAX),
                Value::Text("integer".into()),
                Value::Text("integer".into()),
            ],
            vec![
                Value::Int(2),
                Value::Float(1.25),
                Value::Text("real".into()),
                Value::Text("real".into()),
            ],
            vec![
                Value::Int(3),
                Value::Text("42".into()),
                Value::Text("text".into()),
                Value::Text("text".into()),
            ],
            vec![
                Value::Int(4),
                Value::Int(1),
                Value::Text("integer".into()),
                Value::Text("true".into()),
            ],
            vec![
                Value::Int(5),
                Value::Null,
                Value::Text("null".into()),
                Value::Text("null".into()),
            ],
            vec![
                Value::Int(6),
                Value::Text("{\"x\":1}".into()),
                Value::Text("text".into()),
                Value::Text("object".into()),
            ],
            vec![
                Value::Int(7),
                Value::Text("[1,2]".into()),
                Value::Text("text".into()),
                Value::Text("array".into()),
            ],
            vec![
                Value::Int(8),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
                Value::Text("text".into()),
                Value::Text("text".into()),
            ],
            vec![Value::Int(9), Value::Null, Value::Text("null".into()), Value::Null,],
        ]
    );

    let directory = tempfile::tempdir().unwrap();
    let json_path = directory.path().join("sqlite-json-extract-any.json");
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
    assert_eq!(json[0]["result"], i64::MAX);
    assert_eq!(json[1]["result"], 1.25);
    assert_eq!(json[2]["result"], "42");
    assert_eq!(json[3]["result"], 1);
    assert!(json[4]["result"].is_null());
    assert_eq!(json[5]["result"], "{\"x\":1}");
    assert_eq!(json[6]["result"], "[1,2]");
    assert_eq!(json[7]["result"], "bookie:sqlite-any:v1:integer:9");
    assert!(json[8]["result"].is_null());
    assert_eq!(json[4]["json_kind"], "null");
    assert!(json[8]["json_kind"].is_null());

    let xlsx_path = directory.path().join("sqlite-json-extract-any.xlsx");
    tablepro_core::export::write_result_file(
        &xlsx_path,
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
    let mut archive = zip::ZipArchive::new(std::fs::File::open(xlsx_path).unwrap()).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
    let mut shared_strings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("xl/sharedStrings.xml").unwrap(),
        &mut shared_strings,
    )
    .unwrap();
    assert!(sheet.contains("<c r=\"B2\" t=\"s\">"), "{sheet}");
    assert!(sheet.contains("<c r=\"B3\"><v>1.25</v></c>"), "{sheet}");
    assert!(sheet.contains("<c r=\"B4\" t=\"s\">"), "{sheet}");
    assert!(sheet.contains("<c r=\"B5\"><v>1</v></c>"), "{sheet}");
    for text in [
        "9223372036854775807",
        "42",
        "{\"x\":1}",
        "[1,2]",
        "bookie:sqlite-any:v1:integer:9",
    ] {
        assert!(shared_strings.contains(&format!("<t>{text}</t>")), "{shared_strings}");
    }
    assert!(!sheet.contains("<f>"), "{sheet}");

    sqlite_result_csv_round_trip(
        connection.as_ref(),
        &result,
        "restored",
        &[None, Some(0), Some(1), Some(2), Some(3)],
    )
    .await;

    let restored = connection
        .query("SELECT typeof(result), result, storage_class, json_kind FROM restored ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![
                Value::Text("integer".into()),
                Value::Int(i64::MAX),
                Value::Text("integer".into()),
                Value::Text("integer".into()),
            ],
            vec![
                Value::Text("real".into()),
                Value::Float(1.25),
                Value::Text("real".into()),
                Value::Text("real".into()),
            ],
            vec![
                Value::Text("text".into()),
                Value::Text("42".into()),
                Value::Text("text".into()),
                Value::Text("text".into()),
            ],
            vec![
                Value::Text("integer".into()),
                Value::Int(1),
                Value::Text("integer".into()),
                Value::Text("true".into()),
            ],
            vec![
                Value::Text("null".into()),
                Value::Null,
                Value::Text("null".into()),
                Value::Text("null".into()),
            ],
            vec![
                Value::Text("text".into()),
                Value::Text("{\"x\":1}".into()),
                Value::Text("text".into()),
                Value::Text("object".into()),
            ],
            vec![
                Value::Text("text".into()),
                Value::Text("[1,2]".into()),
                Value::Text("text".into()),
                Value::Text("array".into()),
            ],
            vec![
                Value::Text("text".into()),
                Value::Text("bookie:sqlite-any:v1:integer:9".into()),
                Value::Text("text".into()),
                Value::Text("text".into()),
            ],
            vec![
                Value::Text("null".into()),
                Value::Null,
                Value::Text("null".into()),
                Value::Null,
            ],
        ]
    );
}
