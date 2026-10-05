#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{ConnectOptions, Connection, Value};

use crate::{connect, start_mariadb, start_mysql};

const MODES: [&str; 12] = [
    "",
    "NO_BACKSLASH_ESCAPES",
    "ANSI_QUOTES",
    "ANSI_QUOTES,NO_BACKSLASH_ESCAPES",
    "STRICT_TRANS_TABLES",
    "STRICT_TRANS_TABLES,NO_BACKSLASH_ESCAPES",
    "STRICT_TRANS_TABLES,ANSI_QUOTES",
    "STRICT_TRANS_TABLES,ANSI_QUOTES,NO_BACKSLASH_ESCAPES",
    "STRICT_ALL_TABLES",
    "STRICT_ALL_TABLES,NO_BACKSLASH_ESCAPES",
    "STRICT_ALL_TABLES,ANSI_QUOTES",
    "STRICT_ALL_TABLES,ANSI_QUOTES,NO_BACKSLASH_ESCAPES",
];

async fn connect_in_mode(options: &ConnectOptions, mode: &str) -> Box<dyn Connection> {
    let admin = connect(options.clone()).await;
    admin.execute(&format!("SET GLOBAL sql_mode = '{mode}'")).await.unwrap();
    admin.close().await.unwrap();
    let connection = connect(options.clone()).await;
    let active = connection.query("SELECT @@SESSION.sql_mode").await.unwrap().rows[0][0].clone();
    let Value::Text(active) = active else {
        panic!("unexpected sql_mode: {active:?}")
    };
    assert_eq!(
        active.split(',').any(|value| value.trim() == "NO_BACKSLASH_ESCAPES"),
        mode.split(',').any(|value| value.trim() == "NO_BACKSLASH_ESCAPES"),
        "requested sql_mode {mode:?}, got {active:?}"
    );
    assert_eq!(
        active.split(',').any(|value| value.trim() == "ANSI_QUOTES"),
        mode.split(',').any(|value| value.trim() == "ANSI_QUOTES"),
        "ANSI_QUOTES mode mismatch: {active}"
    );
    assert_eq!(
        active.split(',').any(|value| value.trim() == "STRICT_TRANS_TABLES"),
        mode.split(',').any(|value| value.trim() == "STRICT_TRANS_TABLES"),
        "STRICT_TRANS_TABLES mode mismatch: {active}"
    );
    assert_eq!(
        active.split(',').any(|value| value.trim() == "STRICT_ALL_TABLES"),
        mode.split(',').any(|value| value.trim() == "STRICT_ALL_TABLES"),
        "STRICT_ALL_TABLES mode mismatch: {active}"
    );
    connection
}

async fn assert_enum_and_set_export_modes(options: ConnectOptions) {
    let setup = connect(options.clone()).await;
    setup
        .execute(
            r#"CREATE TABLE enum_mode_source (
                id INT PRIMARY KEY,
                mood ENUM('happy', 'it''s ok', 'back\\slash', 'NULL', '', '<tag>&'),
                perms SET('read', 'write', 'slash\\path', 'NULL', '<member>')
            )"#,
        )
        .await
        .unwrap();
    setup
        .execute("CREATE TABLE enum_mode_copy LIKE enum_mode_source")
        .await
        .unwrap();
    setup
        .execute("CREATE TABLE enum_csv_copy LIKE enum_mode_source")
        .await
        .unwrap();
    for (id, label, permissions) in [
        (1, Some("happy"), Some("read")),
        (2, Some("it's ok"), Some("write,slash\\path")),
        (3, Some("back\\slash"), Some("NULL")),
        (4, Some("NULL"), Some("")),
        (5, Some(""), Some("slash\\path")),
        (6, None, None),
        (7, Some("<tag>&"), Some("<member>")),
    ] {
        setup
            .execute_params(
                "INSERT INTO enum_mode_source VALUES (?, ?, ?)",
                &[
                    Value::Int(id),
                    label.map_or(Value::Null, |value| Value::Text(value.into())),
                    permissions.map_or(Value::Null, |value| Value::Text(value.into())),
                ],
            )
            .await
            .unwrap();
    }
    setup.close().await.unwrap();

    for (suffix, mode) in MODES.into_iter().enumerate() {
        let connection = connect_in_mode(&options, mode).await;
        connection.execute("DELETE FROM enum_mode_copy").await.unwrap();
        connection.execute("DELETE FROM enum_csv_copy").await.unwrap();
        let source = connection
            .query("SELECT id, mood, perms FROM enum_mode_source ORDER BY id")
            .await
            .unwrap();
        let native = connection
            .query(
                "SELECT id, CAST(mood + 0 AS CHAR), HEX(mood), \
                 CAST(perms + 0 AS CHAR), HEX(perms) FROM enum_mode_source ORDER BY id",
            )
            .await
            .unwrap();
        assert_eq!(
            native.rows,
            vec![
                vec![
                    Value::Int(1),
                    Value::Text("1".into()),
                    Value::Text("6861707079".into()),
                    Value::Text("1".into()),
                    Value::Text("72656164".into()),
                ],
                vec![
                    Value::Int(2),
                    Value::Text("2".into()),
                    Value::Text("69742773206F6B".into()),
                    Value::Text("6".into()),
                    Value::Text("77726974652C736C6173685C70617468".into()),
                ],
                vec![
                    Value::Int(3),
                    Value::Text("3".into()),
                    Value::Text("6261636B5C736C617368".into()),
                    Value::Text("8".into()),
                    Value::Text("4E554C4C".into()),
                ],
                vec![
                    Value::Int(4),
                    Value::Text("4".into()),
                    Value::Text("4E554C4C".into()),
                    Value::Text("0".into()),
                    Value::Text(String::new()),
                ],
                vec![
                    Value::Int(5),
                    Value::Text("5".into()),
                    Value::Text(String::new()),
                    Value::Text("4".into()),
                    Value::Text("736C6173685C70617468".into()),
                ],
                vec![Value::Int(6), Value::Null, Value::Null, Value::Null, Value::Null],
                vec![
                    Value::Int(7),
                    Value::Text("6".into()),
                    Value::Text("3C7461673E26".into()),
                    Value::Text("16".into()),
                    Value::Text("3C6D656D6265723E".into()),
                ],
            ],
            "native ENUM ordinals and labels in sql_mode {mode:?}"
        );
        let json: serde_json::Value =
            serde_json::from_str(&tablepro_core::export::render_json(&source.columns, &source.rows)).unwrap();
        assert_eq!(
            json,
            serde_json::json!([
                {"id": 1, "mood": "happy", "perms": "read"},
                {"id": 2, "mood": "it's ok", "perms": "write,slash\\path"},
                {"id": 3, "mood": "back\\slash", "perms": "NULL"},
                {"id": 4, "mood": "NULL", "perms": ""},
                {"id": 5, "mood": "", "perms": "slash\\path"},
                {"id": 6, "mood": null, "perms": null},
                {"id": 7, "mood": "<tag>&", "perms": "<member>"}
            ]),
            "JSON export under sql_mode {mode:?}"
        );
        let csv_options = tablepro_core::export::CsvOptions::default();
        for (extension, format) in [
            ("xml", tablepro_core::export::ResultFormat::Xml),
            ("html", tablepro_core::export::ResultFormat::Html),
            ("md", tablepro_core::export::ResultFormat::Markdown),
        ] {
            let path = std::env::temp_dir().join(format!(
                "tablepro-enum-set-{}-{:?}-{suffix}.{extension}",
                std::process::id(),
                std::thread::current().id()
            ));
            tablepro_core::export::write_result_file(
                &path,
                &source,
                &tablepro_core::export::ResultExport {
                    format,
                    csv: &csv_options,
                    sql: None,
                },
                || false,
                |_| {},
            )
            .unwrap();
            let output = std::fs::read_to_string(&path).unwrap();
            assert!(output.contains("&lt;tag&gt;&amp;"), "{extension}: {output}");
            assert!(output.contains("&lt;member&gt;"), "{extension}: {output}");
            assert!(!output.contains("<tag>&"), "{extension}: {output}");
            assert!(!output.contains("<member>"), "{extension}: {output}");
            std::fs::remove_file(path).unwrap();
        }
        let xlsx_path = std::env::temp_dir().join(format!(
            "tablepro-enum-xlsx-{}-{:?}-{suffix}.xlsx",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&xlsx_path, b"previous workbook").unwrap();
        let csv = tablepro_core::export::CsvOptions::default();
        let xlsx_error = tablepro_core::export::write_result_file(
            &xlsx_path,
            &source,
            &tablepro_core::export::ResultExport {
                format: tablepro_core::export::ResultFormat::Xlsx,
                csv: &csv,
                sql: None,
            },
            || false,
            |_| {},
        )
        .unwrap_err();
        assert!(
            matches!(
                xlsx_error,
                tablepro_core::export::ExportError::WorkbookEmptyText { row: 4, column: 3 }
            ),
            "sql_mode {mode:?}: {xlsx_error:?}"
        );
        assert_eq!(std::fs::read(&xlsx_path).unwrap(), b"previous workbook");
        std::fs::remove_file(xlsx_path).unwrap();

        let null_marker = tablepro_core::export::unique_csv_null_marker(&source.rows);
        let csv_options = tablepro_core::export::CsvOptions {
            null_to_empty: false,
            null_marker: Some(null_marker.clone()),
            ..tablepro_core::export::CsvOptions::default()
        };
        let csv = tablepro_core::export::render_csv(&source.columns, &source.rows, &csv_options);
        let import_options = tablepro_core::import::CsvImportOptions {
            null_marker,
            ..tablepro_core::import::CsvImportOptions::default()
        };
        let sheet = tablepro_core::import::read_csv(csv.as_bytes(), &import_options, None).unwrap();
        let columns = connection.fetch_columns(None, "enum_csv_copy").await.unwrap();
        let mapping = [Some(0), Some(1), Some(2)];
        let plan = tablepro_core::import::build_insert_plan(
            &tablepro_core::import::ImportTarget {
                driver_id: "mysql",
                schema: None,
                table: "enum_csv_copy",
                columns: &columns,
                mapping: &mapping,
            },
            &sheet,
            &import_options,
        )
        .unwrap();
        for row in &plan.rows {
            connection.execute_params(&plan.statement, row).await.unwrap();
        }

        let path = std::env::temp_dir().join(format!(
            "tablepro-enum-sql-{}-{:?}-{suffix}.sql",
            std::process::id(),
            std::thread::current().id()
        ));
        let csv = tablepro_core::export::CsvOptions::default();
        tablepro_core::export::write_result_file(
            &path,
            &source,
            &tablepro_core::export::ResultExport {
                format: tablepro_core::export::ResultFormat::Sql,
                csv: &csv,
                sql: Some(tablepro_core::export::SqlTarget {
                    driver_id: "mysql",
                    schema: None,
                    table: "enum_mode_copy",
                }),
            },
            || false,
            |_| {},
        )
        .unwrap();
        let sql = std::fs::read_to_string(&path).unwrap();
        for statement in sql.lines() {
            connection
                .execute(statement)
                .await
                .unwrap_or_else(|error| panic!("sql_mode {mode:?}; {statement}: {error}"));
        }
        std::fs::remove_file(path).unwrap();
        let matching = connection
            .query(
                "SELECT COUNT(*) FROM enum_mode_source s \
                 JOIN enum_mode_copy f ON s.id = f.id \
                   AND s.mood + 0 <=> f.mood + 0 AND HEX(s.mood) <=> HEX(f.mood) \
                   AND s.perms + 0 <=> f.perms + 0 AND HEX(s.perms) <=> HEX(f.perms) \
                 JOIN enum_csv_copy c ON s.id = c.id \
                   AND s.mood + 0 <=> c.mood + 0 AND HEX(s.mood) <=> HEX(c.mood) \
                   AND s.perms + 0 <=> c.perms + 0 AND HEX(s.perms) <=> HEX(c.perms)",
            )
            .await
            .unwrap();
        assert_eq!(matching.rows, vec![vec![Value::Int(7)]], "sql_mode {mode:?}");
        connection.close().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_enum_and_set_consumers_survive_mysql_sql_modes() {
    let (_container, options) = start_mysql().await;
    assert_enum_and_set_export_modes(options).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_enum_and_set_consumers_survive_mariadb_sql_modes() {
    let (_container, options) = start_mariadb().await;
    assert_enum_and_set_export_modes(options).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_mariadb_empty_enum_and_set_csv_restore_survive_empty_string_is_null() {
    let (_container, options) = start_mariadb().await;
    let setup = connect(options.clone()).await;
    setup
        .execute(
            "CREATE TABLE enum_empty_mode_source (id INT PRIMARY KEY, \
             state ENUM('', 'ready', 'NULL'), permissions SET('read', 'write'))",
        )
        .await
        .unwrap();
    setup
        .execute("CREATE TABLE enum_empty_mode_target LIKE enum_empty_mode_source")
        .await
        .unwrap();
    setup
        .execute(
            "INSERT INTO enum_empty_mode_source VALUES \
             (1, '', ''), (2, 'NULL', 'read'), (3, NULL, NULL)",
        )
        .await
        .unwrap();
    setup
        .execute("INSERT INTO enum_empty_mode_target VALUES (99, 'ready', 'write')")
        .await
        .unwrap();
    setup
        .execute("SET GLOBAL sql_mode = 'EMPTY_STRING_IS_NULL'")
        .await
        .unwrap();
    setup.close().await.unwrap();

    let connection = connect(options).await;
    let active_mode = connection.query("SELECT @@SESSION.sql_mode").await.unwrap();
    let Value::Text(active_mode) = &active_mode.rows[0][0] else {
        panic!("unexpected sql_mode: {:?}", active_mode.rows)
    };
    assert!(
        active_mode.split(',').any(|mode| mode.trim() == "EMPTY_STRING_IS_NULL"),
        "EMPTY_STRING_IS_NULL must be active; got {active_mode}"
    );
    assert_eq!(
        connection
            .query("SELECT HEX(SPACE(0)), SPACE(0) IS NULL")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Text(String::new()), Value::Int(0)]],
        "a server expression can represent empty text without the SQL mode turning it into NULL"
    );
    let source = connection
        .query("SELECT id, state, permissions FROM enum_empty_mode_source ORDER BY id")
        .await
        .unwrap();
    let source_native = connection
        .query(
            "SELECT id, CAST(state + 0 AS CHAR), HEX(state), state IS NULL, \
             CAST(permissions + 0 AS CHAR), HEX(permissions), permissions IS NULL \
             FROM enum_empty_mode_source ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        source_native.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("1".into()),
                Value::Text(String::new()),
                Value::Int(0),
                Value::Text("0".into()),
                Value::Text(String::new()),
                Value::Int(0),
            ],
            vec![
                Value::Int(2),
                Value::Text("3".into()),
                Value::Text("4E554C4C".into()),
                Value::Int(0),
                Value::Text("1".into()),
                Value::Text("72656164".into()),
                Value::Int(0),
            ],
            vec![
                Value::Int(3),
                Value::Null,
                Value::Null,
                Value::Int(1),
                Value::Null,
                Value::Null,
                Value::Int(1),
            ],
        ]
    );

    let null_marker = tablepro_core::export::unique_csv_null_marker(&source.rows);
    let csv_options = tablepro_core::export::CsvOptions {
        null_to_empty: false,
        null_marker: Some(null_marker.clone()),
        ..Default::default()
    };
    let csv = tablepro_core::export::render_csv(&source.columns, &source.rows, &csv_options);
    let import_options = tablepro_core::import::CsvImportOptions {
        null_marker,
        ..Default::default()
    };
    let sheet = tablepro_core::import::read_csv(csv.as_bytes(), &import_options, None).unwrap();
    let columns = connection.fetch_columns(None, "enum_empty_mode_target").await.unwrap();
    let plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "mysql",
            schema: None,
            table: "enum_empty_mode_target",
            columns: &columns,
            mapping: &[Some(0), Some(1), Some(2)],
        },
        &sheet,
        &import_options,
    )
    .unwrap();
    for row in &plan.rows {
        connection.execute_params(&plan.statement, row).await.unwrap();
    }

    let restored = connection
        .query(
            "SELECT id, CAST(state + 0 AS CHAR), HEX(state), state IS NULL, \
             CAST(permissions + 0 AS CHAR), HEX(permissions), permissions IS NULL \
             FROM enum_empty_mode_target ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("1".into()),
                Value::Text(String::new()),
                Value::Int(0),
                Value::Text("0".into()),
                Value::Text(String::new()),
                Value::Int(0),
            ],
            vec![
                Value::Int(2),
                Value::Text("3".into()),
                Value::Text("4E554C4C".into()),
                Value::Int(0),
                Value::Text("1".into()),
                Value::Text("72656164".into()),
                Value::Int(0),
            ],
            vec![
                Value::Int(3),
                Value::Null,
                Value::Null,
                Value::Int(1),
                Value::Null,
                Value::Null,
                Value::Int(1),
            ],
            vec![
                Value::Int(99),
                Value::Text("2".into()),
                Value::Text("7265616479".into()),
                Value::Int(0),
                Value::Text("2".into()),
                Value::Text("7772697465".into()),
                Value::Int(0),
            ],
        ]
    );
    connection.close().await.unwrap();
}
