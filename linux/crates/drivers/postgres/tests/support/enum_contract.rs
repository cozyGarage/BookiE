use tablepro_core::Value;
use tablepro_core::{FilterOp, FilterRule, FilterSet, FilterValue};

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_scalar_enum_labels_preserve_exact_text() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE TYPE value_contract_label AS ENUM ('NULL', '東京', 'o''brien')")
        .await
        .unwrap();

    let result = connection
        .query(
            "SELECT label::text AS enum_value, pg_typeof(label)::text AS native_type \
             FROM (VALUES \
             ('NULL'::value_contract_label), \
             ('東京'::value_contract_label), ('o''brien'::value_contract_label)) AS labels(label)",
        )
        .await
        .unwrap();
    let expected = ["NULL", "東京", "o'brien"];
    assert_eq!(
        result.rows.len(),
        expected.len(),
        "the enum text query must return every source row"
    );
    for (row, expected) in result.rows.iter().zip(expected) {
        assert_eq!(
            row,
            &[Value::Text(expected.into()), Value::Text("value_contract_label".into())]
        );
    }
    let null = connection
        .query("SELECT NULL::value_contract_label::text AS label, NULL::text AS oracle")
        .await
        .unwrap();
    assert_eq!(null.rows, vec![vec![Value::Null, Value::Null]]);

    let direct_projection = connection
        .query(
            "SELECT label, pg_typeof(label)::text AS native_type FROM (VALUES \
             ('NULL'::value_contract_label), ('東京'::value_contract_label), \
             ('o''brien'::value_contract_label), (NULL::value_contract_label)) AS labels(label)",
        )
        .await
        .unwrap();
    assert_eq!(
        direct_projection.rows,
        vec![
            vec![Value::Text("NULL".into()), Value::Text("value_contract_label".into())],
            vec![Value::Text("東京".into()), Value::Text("value_contract_label".into())],
            vec![
                Value::Text("o'brien".into()),
                Value::Text("value_contract_label".into())
            ],
            vec![Value::Null, Value::Text("value_contract_label".into())],
        ]
    );

    connection
        .execute("CREATE TABLE value_contract_enum_null_write (id INT PRIMARY KEY, label value_contract_label)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO value_contract_enum_null_write VALUES (1, '東京')")
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE value_contract_enum_null_write_effects (id INT NOT NULL)")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE FUNCTION value_contract_enum_null_write_effect() RETURNS trigger \
             LANGUAGE plpgsql AS $$ BEGIN INSERT INTO value_contract_enum_null_write_effects VALUES (NEW.id); \
             RETURN NEW; END $$",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TRIGGER value_contract_enum_null_write_trigger BEFORE UPDATE \
             ON value_contract_enum_null_write FOR EACH ROW \
             EXECUTE FUNCTION value_contract_enum_null_write_effect()",
        )
        .await
        .unwrap();
    let update_returning = connection
        .query("UPDATE value_contract_enum_null_write SET label = 'NULL' WHERE id = 1 RETURNING label")
        .await
        .unwrap();
    assert_eq!(update_returning.rows, vec![vec![Value::Text("NULL".into())]]);
    let stored = connection
        .query("SELECT label::text, pg_typeof(label)::text FROM value_contract_enum_null_write WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![vec![
            Value::Text("NULL".into()),
            Value::Text("value_contract_label".into()),
        ]]
    );
    let effects = connection
        .query("SELECT count(*)::bigint FROM value_contract_enum_null_write_effects")
        .await
        .unwrap();
    assert_eq!(effects.rows, vec![vec![Value::Int(1)]]);

    for value in ["NULL", "東京", "o'brien"] {
        let input = Value::Text(value.into());
        let literal = tablepro_core::sql_literal::render_sql_literal("postgres", &input).unwrap();
        let literal_result = connection
            .query(&format!("SELECT {literal}::value_contract_label::text"))
            .await
            .unwrap();
        assert_eq!(literal_result.rows, vec![vec![Value::Text(value.into())]]);

        let bound_result = connection
            .query_params(
                "SELECT $1::value_contract_label AS label, \
                 pg_typeof($1::value_contract_label)::text AS native_type",
                std::slice::from_ref(&input),
            )
            .await
            .unwrap();
        assert_eq!(
            bound_result.rows,
            vec![vec![
                Value::Text(value.into()),
                Value::Text("value_contract_label".into())
            ]]
        );
    }
    let bound_null = connection
        .query_params(
            "SELECT $1::value_contract_label AS label, \
             pg_typeof($1::value_contract_label)::text AS native_type",
            &[Value::Null],
        )
        .await
        .unwrap();
    assert_eq!(
        bound_null.rows,
        vec![vec![Value::Null, Value::Text("value_contract_label".into())]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_filters_preserve_labels_and_sql_null() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_enum_filter")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_enum_filter.status AS ENUM ('NULL', 'ready', 'paused', '東京')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_enum_filter.rows (id INT PRIMARY KEY, status value_contract_enum_filter.status)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_enum_filter.rows VALUES \
             (1, 'NULL'), (2, 'ready'), (3, 'paused'), (4, '東京'), (5, NULL)",
        )
        .await
        .unwrap();

    let columns = connection
        .fetch_columns(Some("value_contract_enum_filter"), "rows")
        .await
        .unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "value_contract_enum_filter".into(),
            name: "status".into(),
        })
    );

    let cases = [
        (FilterOp::Eq, FilterValue::Single("NULL".into()), vec![(1, "NULL")]),
        (
            FilterOp::NotEq,
            FilterValue::Single("NULL".into()),
            vec![(2, "ready"), (3, "paused"), (4, "東京")],
        ),
        (
            FilterOp::Lt,
            FilterValue::Single("paused".into()),
            vec![(1, "NULL"), (2, "ready")],
        ),
        (
            FilterOp::LtEq,
            FilterValue::Single("paused".into()),
            vec![(1, "NULL"), (2, "ready"), (3, "paused")],
        ),
        (
            FilterOp::Gt,
            FilterValue::Single("ready".into()),
            vec![(3, "paused"), (4, "東京")],
        ),
        (
            FilterOp::GtEq,
            FilterValue::Single("ready".into()),
            vec![(2, "ready"), (3, "paused"), (4, "東京")],
        ),
        (
            FilterOp::Contains,
            FilterValue::Single("aus".into()),
            vec![(3, "paused")],
        ),
        (
            FilterOp::StartsWith,
            FilterValue::Single("rea".into()),
            vec![(2, "ready")],
        ),
        (FilterOp::EndsWith, FilterValue::Single("dy".into()), vec![(2, "ready")]),
        (FilterOp::Like, FilterValue::Single("pau%".into()), vec![(3, "paused")]),
        (
            FilterOp::NotLike,
            FilterValue::Single("p%".into()),
            vec![(1, "NULL"), (2, "ready"), (4, "東京")],
        ),
        (
            FilterOp::Ilike,
            FilterValue::Single("%READY%".into()),
            vec![(2, "ready")],
        ),
        (
            FilterOp::In,
            FilterValue::List(vec!["NULL".into(), "東京".into()]),
            vec![(1, "NULL"), (4, "東京")],
        ),
        (
            FilterOp::NotIn,
            FilterValue::List(vec!["NULL".into(), "東京".into()]),
            vec![(2, "ready"), (3, "paused")],
        ),
        (
            FilterOp::Between,
            FilterValue::Pair("ready".into(), "paused".into()),
            vec![(2, "ready"), (3, "paused")],
        ),
    ];
    for (op, value, expected_rows) in cases {
        let filter = FilterSet {
            rules: vec![FilterRule {
                column: "status".into(),
                op,
                value: Some(value),
            }],
            ..Default::default()
        };
        let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &filter)
            .unwrap()
            .unwrap();
        let result = connection
            .query_params(
                &format!(
                    "SELECT id, status::text, pg_typeof(status)::text \
                     FROM value_contract_enum_filter.rows WHERE {where_sql} ORDER BY id"
                ),
                &params,
            )
            .await
            .unwrap();
        assert_eq!(result.rows.len(), expected_rows.len());
        for (row, (id, label)) in result.rows.iter().zip(expected_rows) {
            assert_eq!(row[0], Value::Int(id));
            assert_eq!(row[1], Value::Text(label.into()));
            assert_eq!(row[2], Value::Text("value_contract_enum_filter.status".into()));
        }
    }

    let null_filter = FilterSet {
        rules: vec![FilterRule {
            column: "status".into(),
            op: FilterOp::IsNull,
            value: None,
        }],
        ..Default::default()
    };
    let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &null_filter)
        .unwrap()
        .unwrap();
    let result = connection
        .query_params(
            &format!(
                "SELECT id, status::text, pg_typeof(status)::text \
                 FROM value_contract_enum_filter.rows WHERE {where_sql}"
            ),
            &params,
        )
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![vec![
            Value::Int(5),
            Value::Null,
            Value::Text("value_contract_enum_filter.status".into()),
        ]]
    );

    let not_null_filter = FilterSet {
        rules: vec![FilterRule {
            column: "status".into(),
            op: FilterOp::IsNotNull,
            value: None,
        }],
        ..Default::default()
    };
    let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &not_null_filter)
        .unwrap()
        .unwrap();
    let result = connection
        .query_params(
            &format!(
                "SELECT id, status::text, pg_typeof(status)::text \
                 FROM value_contract_enum_filter.rows WHERE {where_sql} ORDER BY id"
            ),
            &params,
        )
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("NULL".into()),
                Value::Text("value_contract_enum_filter.status".into())
            ],
            vec![
                Value::Int(2),
                Value::Text("ready".into()),
                Value::Text("value_contract_enum_filter.status".into())
            ],
            vec![
                Value::Int(3),
                Value::Text("paused".into()),
                Value::Text("value_contract_enum_filter.status".into())
            ],
            vec![
                Value::Int(4),
                Value::Text("東京".into()),
                Value::Text("value_contract_enum_filter.status".into())
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_csv_round_trip_preserves_labels_and_sql_null() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_enum_import")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_enum_import.status AS ENUM ('NULL', '東京', 'o''brien', 'with,comma', '')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_enum_import.source_rows (id INT PRIMARY KEY, \
             status value_contract_enum_import.status, note TEXT NOT NULL)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_enum_import.source_rows VALUES \
             (1, 'NULL', 'literal null'), (2, '東京', 'unicode'), \
             (3, 'o''brien', 'apostrophe'), (4, 'with,comma', 'delimiter'), \
             (5, '', 'empty label'), (6, NULL, 'sql null')",
        )
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_enum_import.target_rows (id INT PRIMARY KEY, \
             status value_contract_enum_import.status, note TEXT NOT NULL)",
        )
        .await
        .unwrap();

    let source = connection
        .query("SELECT id, status, note FROM value_contract_enum_import.source_rows ORDER BY id")
        .await
        .unwrap();
    let options = tablepro_core::import::CsvImportOptions::default();
    let csv = tablepro_core::export::render_csv(
        &source.columns,
        &source.rows,
        &tablepro_core::export::CsvOptions::default(),
    );
    let exported_sheet = tablepro_core::import::read_csv(csv.as_bytes(), &options, None).unwrap();
    assert_eq!(exported_sheet.rows[0], vec!["1", "NULL", "literal null"]);
    assert_eq!(exported_sheet.rows[4], vec!["5", "", "empty label"]);
    assert_eq!(exported_sheet.rows[5], vec!["6", "", "sql null"]);

    let columns = connection
        .fetch_columns(Some("value_contract_enum_import"), "target_rows")
        .await
        .unwrap();
    let default_target = tablepro_core::import::ImportTarget {
        driver_id: "postgres",
        schema: Some("value_contract_enum_import"),
        table: "target_rows",
        columns: &columns,
        mapping: &[Some(0), Some(1), Some(2)],
    };
    let error = tablepro_core::import::build_insert_plan(&default_target, &exported_sheet, &options)
        .expect_err("default CSV blanks cannot distinguish an empty enum label from SQL NULL");
    let tablepro_core::import::PlanError::Rows { total, first } = error else {
        panic!("expected ambiguous enum fields to be refused");
    };
    assert_eq!(total, 2);
    assert!(
        first
            .iter()
            .all(|row| row.reason == tablepro_core::import::CellError::AmbiguousEnumNullOrEmpty)
    );
    let untouched = connection
        .query("SELECT count(*)::bigint FROM value_contract_enum_import.target_rows")
        .await
        .unwrap();
    assert_eq!(untouched.rows, vec![vec![Value::Int(0)]]);

    let explicit_csv = "id,status,note\n1,NULL,literal null\n2,東京,unicode\n3,\"o'brien\",apostrophe\n4,\"with,comma\",delimiter\n5,,empty label\n6,\\N,sql null\n";
    let options = tablepro_core::import::CsvImportOptions {
        null_marker: "\\N".into(),
        ..Default::default()
    };
    let sheet = tablepro_core::import::read_csv(explicit_csv.as_bytes(), &options, None).unwrap();
    let plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "postgres",
            schema: Some("value_contract_enum_import"),
            table: "target_rows",
            columns: &columns,
            mapping: &[Some(0), Some(1), Some(2)],
        },
        &sheet,
        &options,
    )
    .unwrap();
    assert!(
        plan.statement
            .contains("$2::text::\"value_contract_enum_import\".\"status\""),
        "enum CSV input must bind with its catalog type: {}",
        plan.statement
    );
    assert_eq!(plan.rows.len(), 6);
    for row in &plan.rows {
        connection.execute_params(&plan.statement, row).await.unwrap();
    }

    let restored = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text, note \
             FROM value_contract_enum_import.target_rows ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("NULL".into()),
                Value::Text("value_contract_enum_import.status".into()),
                Value::Text("literal null".into()),
            ],
            vec![
                Value::Int(2),
                Value::Text("東京".into()),
                Value::Text("value_contract_enum_import.status".into()),
                Value::Text("unicode".into()),
            ],
            vec![
                Value::Int(3),
                Value::Text("o'brien".into()),
                Value::Text("value_contract_enum_import.status".into()),
                Value::Text("apostrophe".into()),
            ],
            vec![
                Value::Int(4),
                Value::Text("with,comma".into()),
                Value::Text("value_contract_enum_import.status".into()),
                Value::Text("delimiter".into()),
            ],
            vec![
                Value::Int(5),
                Value::Text(String::new()),
                Value::Text("value_contract_enum_import.status".into()),
                Value::Text("empty label".into()),
            ],
            vec![
                Value::Int(6),
                Value::Null,
                Value::Text("value_contract_enum_import.status".into()),
                Value::Text("sql null".into()),
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_enum_null_label_and_sql_null_write_as_distinct_values() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE TYPE value_contract_null_write_enum AS ENUM ('ready', 'NULL')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_null_enum_edits (id INT PRIMARY KEY, \
             status value_contract_null_write_enum, sibling TEXT NOT NULL)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_null_enum_edits VALUES \
             (1, 'ready', 'left'), (2, 'ready', 'right')",
        )
        .await
        .unwrap();

    let columns = connection
        .fetch_columns(None, "value_contract_null_enum_edits")
        .await
        .unwrap();
    let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "value_contract_null_enum_edits",
        &columns,
        &[(1, Value::Text("NULL".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    connection.execute_params(&sql, &params).await.unwrap();
    let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "value_contract_null_enum_edits",
        &columns,
        &[(1, Value::Null)],
        &[Value::Int(2)],
    )
    .unwrap();
    connection.execute_params(&sql, &params).await.unwrap();

    let result = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text AS native_type, sibling \
             FROM value_contract_null_enum_edits ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("NULL".into()),
                Value::Text("value_contract_null_write_enum".into()),
                Value::Text("left".into()),
            ],
            vec![
                Value::Int(2),
                Value::Null,
                Value::Text("value_contract_null_write_enum".into()),
                Value::Text("right".into()),
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_projection_preserves_labels_and_null() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE TYPE value_contract_plain_enum AS ENUM ('ready', 'paused')")
        .await
        .unwrap();

    let result = connection
        .query(
            "SELECT label, pg_typeof(label)::text AS native_type \
             FROM (VALUES ('ready'::value_contract_plain_enum), \
             ('paused'::value_contract_plain_enum), \
             (NULL::value_contract_plain_enum)) AS labels(label)",
        )
        .await
        .unwrap();

    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Text("ready".into()),
                Value::Text("value_contract_plain_enum".into())
            ],
            vec![
                Value::Text("paused".into()),
                Value::Text("value_contract_plain_enum".into())
            ],
            vec![Value::Null, Value::Text("value_contract_plain_enum".into())],
        ]
    );

    let array = connection
        .query(
            "SELECT labels, pg_typeof(labels)::text AS native_type \
             FROM (SELECT ARRAY['ready'::value_contract_plain_enum, \
             'paused'::value_contract_plain_enum, NULL::value_contract_plain_enum] AS labels) source",
        )
        .await
        .unwrap();
    assert_eq!(
        array.rows,
        vec![vec![
            Value::Text(r#"{"ready","paused",NULL}"#.into()),
            Value::Text("value_contract_plain_enum[]".into()),
        ]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_json_and_file_exports_preserve_empty_literal_null_and_sql_null() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE TYPE value_contract_json_enum AS ENUM ('NULL', '', '東京')")
        .await
        .unwrap();

    let result = connection
        .query(
            "SELECT label, pg_typeof(label)::text AS native_type FROM (VALUES \
             ('NULL'::value_contract_json_enum), (''::value_contract_json_enum), \
             ('東京'::value_contract_json_enum), (NULL::value_contract_json_enum)) AS labels(label)",
        )
        .await
        .unwrap();
    assert_eq!(result.rows.len(), 4, "the JSON input must retain all enum rows");
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Text("NULL".into()),
                Value::Text("value_contract_json_enum".into())
            ],
            vec![
                Value::Text(String::new()),
                Value::Text("value_contract_json_enum".into())
            ],
            vec![
                Value::Text("東京".into()),
                Value::Text("value_contract_json_enum".into())
            ],
            vec![Value::Null, Value::Text("value_contract_json_enum".into())],
        ]
    );

    let exported = tablepro_core::export::render_json(&result.columns, &result.rows);
    let json: serde_json::Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(
        json,
        serde_json::json!([
            {"label": "NULL", "native_type": "value_contract_json_enum"},
            {"label": "", "native_type": "value_contract_json_enum"},
            {"label": "東京", "native_type": "value_contract_json_enum"},
            {"label": null, "native_type": "value_contract_json_enum"}
        ])
    );

    let directory = tempfile::tempdir().unwrap();
    let json_path = directory.path().join("enum.json");
    let csv_path = directory.path().join("enum.csv");
    let csv_options = tablepro_core::export::CsvOptions::default();
    for (path, format) in [
        (&json_path, tablepro_core::export::ResultFormat::Json),
        (&csv_path, tablepro_core::export::ResultFormat::Csv),
    ] {
        tablepro_core::export::write_result_file(
            path,
            &result,
            &tablepro_core::export::ResultExport {
                format,
                csv: &csv_options,
                sql: None,
            },
            || false,
            |_| {},
        )
        .unwrap();
    }

    let file_json: serde_json::Value = serde_json::from_slice(&std::fs::read(&json_path).unwrap()).unwrap();
    assert_eq!(file_json, json);
    assert_eq!(
        std::fs::read_to_string(&csv_path).unwrap(),
        "label,native_type\nNULL,value_contract_json_enum\n\"\",value_contract_json_enum\n東京,value_contract_json_enum\n,value_contract_json_enum\n"
    );

    let previous = std::fs::read(&json_path).unwrap();
    let cancelled = std::cell::Cell::new(false);
    let error = tablepro_core::export::write_result_file(
        &json_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Json,
            csv: &csv_options,
            sql: None,
        },
        || cancelled.get(),
        |_| cancelled.set(true),
    )
    .unwrap_err();
    assert!(error.is_cancelled(), "{error:?}");
    assert_eq!(std::fs::read(&json_path).unwrap(), previous);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_sql_file_export_restores_labels_and_native_type() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute(
            "CREATE TYPE value_contract_sql_file_enum AS ENUM \
             ('NULL', '', '東京', 'x''; DROP TABLE value_contract_sql_file_restore; --')",
        )
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE value_contract_sql_file_source (id INT PRIMARY KEY, label value_contract_sql_file_enum)")
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_sql_file_source VALUES \
             (1, 'NULL'), (2, ''), (3, '東京'), \
             (4, 'x''; DROP TABLE value_contract_sql_file_restore; --'), (5, NULL)",
        )
        .await
        .unwrap();

    let result = connection
        .query("SELECT id, label FROM value_contract_sql_file_source ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![
            vec![Value::Int(1), Value::Text("NULL".into())],
            vec![Value::Int(2), Value::Text(String::new())],
            vec![Value::Int(3), Value::Text("東京".into())],
            vec![
                Value::Int(4),
                Value::Text("x'; DROP TABLE value_contract_sql_file_restore; --".into()),
            ],
            vec![Value::Int(5), Value::Null],
        ]
    );

    let directory = tempfile::tempdir().unwrap();
    let sql_path = directory.path().join("enum.sql");
    let csv_options = tablepro_core::export::CsvOptions::default();
    tablepro_core::export::write_result_file(
        &sql_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Sql,
            csv: &csv_options,
            sql: Some(tablepro_core::export::SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "value_contract_sql_file_restore",
            }),
        },
        || false,
        |_| {},
    )
    .unwrap();
    let sql = std::fs::read_to_string(&sql_path).unwrap();
    assert_eq!(sql.lines().count(), result.rows.len());
    assert!(sql.contains("x''; DROP TABLE value_contract_sql_file_restore; --"));

    connection
        .execute(
            "CREATE TABLE value_contract_sql_file_restore (id INT PRIMARY KEY, label value_contract_sql_file_enum)",
        )
        .await
        .unwrap();
    for statement in sql.lines() {
        connection.execute(statement).await.unwrap();
    }
    let restored = connection
        .query(
            "SELECT id, label, pg_typeof(label)::text AS native_type \
             FROM value_contract_sql_file_restore ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        restored.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("NULL".into()),
                Value::Text("value_contract_sql_file_enum".into()),
            ],
            vec![
                Value::Int(2),
                Value::Text(String::new()),
                Value::Text("value_contract_sql_file_enum".into()),
            ],
            vec![
                Value::Int(3),
                Value::Text("東京".into()),
                Value::Text("value_contract_sql_file_enum".into()),
            ],
            vec![
                Value::Int(4),
                Value::Text("x'; DROP TABLE value_contract_sql_file_restore; --".into()),
                Value::Text("value_contract_sql_file_enum".into()),
            ],
            vec![
                Value::Int(5),
                Value::Null,
                Value::Text("value_contract_sql_file_enum".into()),
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_parameters_infer_native_type_for_query_and_write() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_enum_bind")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_enum_bind.state AS ENUM ('NULL', '東京', 'ready')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_enum_bind.rows (id INT PRIMARY KEY, status value_contract_enum_bind.state)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_enum_bind.rows VALUES \
             (1, 'NULL'), (2, NULL), (3, '東京')",
        )
        .await
        .unwrap();

    let literal_null = connection
        .query_params(
            "SELECT id, status::text, pg_typeof(status)::text FROM value_contract_enum_bind.rows \
             WHERE status = $1 ORDER BY id",
            &[Value::Text("NULL".into())],
        )
        .await
        .unwrap();
    assert_eq!(
        literal_null.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("NULL".into()),
            Value::Text("value_contract_enum_bind.state".into())
        ]]
    );

    let sql_null = connection
        .query_params(
            "SELECT id, pg_typeof(status)::text FROM value_contract_enum_bind.rows \
             WHERE status IS NOT DISTINCT FROM $1 ORDER BY id",
            &[Value::Null],
        )
        .await
        .unwrap();
    assert_eq!(
        sql_null.rows,
        vec![vec![
            Value::Int(2),
            Value::Text("value_contract_enum_bind.state".into())
        ]]
    );

    let updated = connection
        .execute_params(
            "UPDATE value_contract_enum_bind.rows SET status = $1 WHERE id = 3",
            &[Value::Text("ready".into())],
        )
        .await
        .unwrap();
    assert_eq!(updated.rows_affected, 1);
    let stored = connection
        .query(
            "SELECT status::text, pg_typeof(status)::text FROM value_contract_enum_bind.rows \
             WHERE id = 3",
        )
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![vec![
            Value::Text("ready".into()),
            Value::Text("value_contract_enum_bind.state".into())
        ]]
    );

    let direct_null = connection
        .execute_params(
            "UPDATE value_contract_enum_bind.rows SET status = $1 WHERE id = 3",
            &[Value::Null],
        )
        .await
        .unwrap();
    assert_eq!(direct_null.rows_affected, 1);
    let direct_null_value = connection
        .query(
            "SELECT status::text, pg_typeof(status)::text FROM value_contract_enum_bind.rows \
             WHERE id = 3",
        )
        .await
        .unwrap();
    assert_eq!(
        direct_null_value.rows,
        vec![vec![Value::Null, Value::Text("value_contract_enum_bind.state".into())]]
    );

    let transaction_results = connection
        .execute_in_transaction(&[
            (
                "UPDATE value_contract_enum_bind.rows SET status = $1 WHERE id = 1".into(),
                vec![Value::Text("東京".into())],
            ),
            (
                "UPDATE value_contract_enum_bind.rows SET status = $1 WHERE id = 3".into(),
                vec![Value::Null],
            ),
        ])
        .await
        .unwrap();
    assert_eq!(transaction_results, vec![1, 1]);
    let transaction_values = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text FROM value_contract_enum_bind.rows \
             WHERE id IN (1, 3) ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        transaction_values.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("東京".into()),
                Value::Text("value_contract_enum_bind.state".into())
            ],
            vec![
                Value::Int(3),
                Value::Null,
                Value::Text("value_contract_enum_bind.state".into())
            ],
        ]
    );

    let invalid = connection
        .query_params(
            "SELECT id FROM value_contract_enum_bind.rows WHERE status = $1",
            &[Value::Text("missing".into())],
        )
        .await
        .unwrap_err();
    assert!(matches!(
        invalid,
        tablepro_core::DriverError::Query { sqlstate: Some(code), .. } if code == "22P02"
    ));

    let plain_text = connection
        .query_params("SELECT $1, pg_typeof($1)::text", &[Value::Text("plain text".into())])
        .await
        .unwrap();
    assert_eq!(
        plain_text.rows,
        vec![vec![Value::Text("plain text".into()), Value::Text("text".into())]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_keyed_edit_preserves_label_and_siblings() {
    let (_container, opts) = start_pg().await;
    let connection = connect(opts).await;
    connection
        .execute("CREATE SCHEMA value_contract_enum_schema")
        .await
        .unwrap();
    connection
        .execute("CREATE TYPE value_contract_enum_schema.value_contract_grid_enum AS ENUM ('ready', 'paused', 'NULL')")
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE value_contract_enum_edits (id INT PRIMARY KEY, \
             status value_contract_enum_schema.value_contract_grid_enum, sibling TEXT NOT NULL)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO value_contract_enum_edits VALUES \
             (1, 'ready', 'left'), (2, 'ready', 'right')",
        )
        .await
        .unwrap();

    let columns = connection
        .fetch_columns(None, "value_contract_enum_edits")
        .await
        .unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "value_contract_enum_schema".into(),
            name: "value_contract_grid_enum".into(),
        })
    );
    let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "value_contract_enum_edits",
        &columns,
        &[(1, Value::Text("paused".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    connection.execute_params(&sql, &params).await.unwrap();
    let insert_columns = [columns[0].clone(), columns[1].clone(), columns[2].clone()];
    let (sql, params) = tablepro_core::sql_dialect::build_insert_from_draft(
        "postgres",
        None,
        "value_contract_enum_edits",
        &insert_columns,
        &[
            Value::Int(3),
            Value::Text("NULL".into()),
            Value::Text("inserted".into()),
        ],
    )
    .unwrap();
    connection.execute_params(&sql, &params).await.unwrap();
    let (sql, params) = tablepro_core::sql_dialect::build_insert_from_draft(
        "postgres",
        None,
        "value_contract_enum_edits",
        &insert_columns,
        &[Value::Int(4), Value::Null, Value::Text("inserted-null".into())],
    )
    .unwrap();
    connection.execute_params(&sql, &params).await.unwrap();
    let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "value_contract_enum_edits",
        &columns,
        &[(1, Value::Null)],
        &[Value::Int(2)],
    )
    .unwrap();
    connection.execute_params(&sql, &params).await.unwrap();

    let result = connection
        .query(
            "SELECT id, status::text, pg_typeof(status)::text AS native_type, sibling \
             FROM value_contract_enum_edits ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("paused".into()),
                Value::Text("value_contract_enum_schema.value_contract_grid_enum".into()),
                Value::Text("left".into()),
            ],
            vec![
                Value::Int(2),
                Value::Null,
                Value::Text("value_contract_enum_schema.value_contract_grid_enum".into()),
                Value::Text("right".into()),
            ],
            vec![
                Value::Int(3),
                Value::Text("NULL".into()),
                Value::Text("value_contract_enum_schema.value_contract_grid_enum".into()),
                Value::Text("inserted".into()),
            ],
            vec![
                Value::Int(4),
                Value::Null,
                Value::Text("value_contract_enum_schema.value_contract_grid_enum".into()),
                Value::Text("inserted-null".into()),
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_keyed_edit_resolves_shadowed_type_name_by_schema() {
    let (_container, opts) = start_pg().await;
    let setup = connect(opts.clone()).await;
    setup.execute("CREATE SCHEMA enum_shadow_a").await.unwrap();
    setup.execute("CREATE SCHEMA enum_shadow_b").await.unwrap();
    setup
        .execute("CREATE TYPE enum_shadow_a.status_kind AS ENUM ('ready', 'paused')")
        .await
        .unwrap();
    setup
        .execute("CREATE TYPE enum_shadow_b.status_kind AS ENUM ('ready', 'paused')")
        .await
        .unwrap();
    setup
        .execute(
            "CREATE TABLE enum_shadow_a.items (id INT PRIMARY KEY, status enum_shadow_a.status_kind, sibling TEXT NOT NULL)",
        )
        .await
        .unwrap();
    setup
        .execute(
            "CREATE TABLE enum_shadow_b.items (id INT PRIMARY KEY, status enum_shadow_b.status_kind, sibling TEXT NOT NULL)",
        )
        .await
        .unwrap();
    setup
        .execute("INSERT INTO enum_shadow_a.items VALUES (1, 'ready', 'shadow')")
        .await
        .unwrap();
    setup
        .execute("INSERT INTO enum_shadow_b.items VALUES (1, 'ready', 'target')")
        .await
        .unwrap();
    setup
        .execute("ALTER ROLE postgres SET search_path TO enum_shadow_a")
        .await
        .unwrap();
    drop(setup);

    let connection = connect(opts).await;
    assert_eq!(
        connection.query("SELECT current_schema()::text").await.unwrap().rows,
        vec![vec![Value::Text("enum_shadow_a".into())]]
    );

    let columns = connection.fetch_columns(Some("enum_shadow_b"), "items").await.unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "enum_shadow_b".into(),
            name: "status_kind".into(),
        })
    );
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        Some("enum_shadow_b"),
        "items",
        &columns,
        &[(1, Value::Text("paused".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    connection.execute_params(&update.0, &update.1).await.unwrap();

    let target = connection
        .query("SELECT id, status::text, pg_typeof(status)::text, sibling FROM enum_shadow_b.items")
        .await
        .unwrap();
    let shadow = connection
        .query("SELECT id, status::text, pg_typeof(status)::text, sibling FROM enum_shadow_a.items")
        .await
        .unwrap();
    assert_eq!(
        target.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("paused".into()),
            Value::Text("enum_shadow_b.status_kind".into()),
            Value::Text("target".into()),
        ]]
    );
    assert_eq!(
        shadow.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("ready".into()),
            Value::Text("status_kind".into()),
            Value::Text("shadow".into()),
        ]]
    );

    let filters = tablepro_core::FilterSet {
        rules: vec![tablepro_core::FilterRule {
            column: "status".into(),
            op: tablepro_core::FilterOp::Eq,
            value: Some(tablepro_core::FilterValue::Single("paused".into())),
        }],
        ..Default::default()
    };
    let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &filters)
        .unwrap()
        .unwrap();
    let filtered = connection
        .query_params(
            &format!(
                "SELECT id, status::text, pg_typeof(status)::text \
                 FROM enum_shadow_b.items WHERE {where_sql} ORDER BY id"
            ),
            &params,
        )
        .await
        .unwrap();
    assert_eq!(
        filtered.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("paused".into()),
            Value::Text("enum_shadow_b.status_kind".into()),
        ]]
    );

    for (operator, value) in [
        (
            tablepro_core::FilterOp::In,
            tablepro_core::FilterValue::List(vec!["ready".into(), "paused".into()]),
        ),
        (
            tablepro_core::FilterOp::Between,
            tablepro_core::FilterValue::Pair("ready".into(), "paused".into()),
        ),
    ] {
        let filters = tablepro_core::FilterSet {
            rules: vec![tablepro_core::FilterRule {
                column: "status".into(),
                op: operator,
                value: Some(value),
            }],
            ..Default::default()
        };
        let (where_sql, params) = tablepro_core::build_filter_where("postgres", &columns, &filters)
            .unwrap()
            .unwrap();
        let filtered = connection
            .query_params(
                &format!(
                    "SELECT id, status::text, pg_typeof(status)::text \
                     FROM enum_shadow_b.items WHERE {where_sql} ORDER BY id"
                ),
                &params,
            )
            .await
            .unwrap();
        assert_eq!(
            filtered.rows,
            vec![vec![
                Value::Int(1),
                Value::Text("paused".into()),
                Value::Text("enum_shadow_b.status_kind".into()),
            ]],
            "operator {operator:?} under shadowed search_path"
        );
    }

    for (operator, placeholders, params) in [
        ("=", "$1", vec![Value::Text("paused".into())]),
        (
            "IN",
            "($1, $2)",
            vec![Value::Text("ready".into()), Value::Text("paused".into())],
        ),
        (
            "BETWEEN",
            "$1 AND $2",
            vec![Value::Text("paused".into()), Value::Text("paused".into())],
        ),
    ] {
        let parameterized = connection
            .query_params(
                &format!(
                    "SELECT id, status::text, pg_typeof(status)::text \
                     FROM enum_shadow_b.items \
                     WHERE status::enum_shadow_b.status_kind {operator} {placeholders} ORDER BY id"
                ),
                &params,
            )
            .await
            .unwrap();
        assert_eq!(
            parameterized.rows,
            vec![vec![
                Value::Int(1),
                Value::Text("paused".into()),
                Value::Text("enum_shadow_b.status_kind".into()),
            ]],
            "direct {operator} parameters under shadowed search_path"
        );
    }

    let shadow_after = connection
        .query("SELECT id, status::text, sibling FROM enum_shadow_a.items")
        .await
        .unwrap();
    assert_eq!(
        shadow_after.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("ready".into()),
            Value::Text("shadow".into()),
        ]]
    );
}
