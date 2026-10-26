#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_csv_import_uses_target_schema_under_shadowed_search_path() {
    let (_container, opts) = start_pg().await;
    let setup = connect(opts.clone()).await;
    setup.execute("CREATE SCHEMA enum_csv_shadow_a").await.unwrap();
    setup.execute("CREATE SCHEMA enum_csv_shadow_b").await.unwrap();
    setup.execute("CREATE SCHEMA enum_csv_shadow_c").await.unwrap();
    setup
        .execute("CREATE TYPE enum_csv_shadow_a.state AS ENUM ('ready', 'shadow-only', 'NULL', '')")
        .await
        .unwrap();
    setup
        .execute("CREATE TYPE enum_csv_shadow_b.state AS ENUM ('ready', 'target-only', 'NULL', '')")
        .await
        .unwrap();
    setup
        .execute("CREATE TYPE enum_csv_shadow_c.state AS ENUM ('ready', 'decoy-only')")
        .await
        .unwrap();
    setup
        .execute(
            "CREATE TABLE enum_csv_shadow_b.source_rows \
             (id integer PRIMARY KEY, state enum_csv_shadow_b.state)",
        )
        .await
        .unwrap();
    setup
        .execute(
            "INSERT INTO enum_csv_shadow_b.source_rows VALUES \
             (1, 'target-only'), (2, 'NULL'), (3, ''), (4, NULL)",
        )
        .await
        .unwrap();
    setup
        .execute(
            "CREATE TABLE enum_csv_shadow_b.target_rows \
             (id integer PRIMARY KEY, state enum_csv_shadow_b.state)",
        )
        .await
        .unwrap();
    setup
        .execute("INSERT INTO enum_csv_shadow_b.target_rows VALUES (99, 'ready')")
        .await
        .unwrap();
    setup
        .execute("CREATE TABLE enum_csv_shadow_a.target_rows (id integer PRIMARY KEY, state text)")
        .await
        .unwrap();
    setup.execute("ALTER ROLE postgres SET search_path TO enum_csv_shadow_a, public").await.unwrap();
    drop(setup);

    let connection = connect(opts.clone()).await;
    assert_eq!(
        connection.query("SELECT current_schema()::text").await.unwrap().rows,
        vec![vec![Value::Text("enum_csv_shadow_a".into())]]
    );
    let source = connection
        .query("SELECT id, state FROM enum_csv_shadow_b.source_rows ORDER BY id")
        .await
        .unwrap();
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
    let columns = connection
        .fetch_columns(Some("enum_csv_shadow_b"), "target_rows")
        .await
        .unwrap();
    assert_eq!(
        columns[1].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "enum_csv_shadow_b".into(),
            name: "state".into(),
        })
    );
    let plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "postgres",

            capabilities: tablepro_core::DriverCapabilities::default(),
            schema: Some("enum_csv_shadow_b"),
            table: "target_rows",
            columns: &columns,
            mapping: &[Some(0), Some(1)],
        },
        &sheet,
        &import_options,
    )
    .unwrap();
    assert!(
        plan.statement
            .contains("$2::text::\"enum_csv_shadow_b\".\"state\""),
        "CSV import must cast to the destination enum despite search_path: {}",
        plan.statement
    );
    let statements = plan
        .rows
        .iter()
        .map(|row| (plan.statement.clone(), row.clone()))
        .collect::<Vec<_>>();

    let setup = connect(opts.clone()).await;
    setup
        .execute("ALTER ROLE postgres SET search_path TO enum_csv_shadow_c, enum_csv_shadow_a, public")
        .await
        .unwrap();
    setup.close().await.unwrap();
    connection.close().await.unwrap();

    let connection = connect(opts).await;
    assert_eq!(
        connection.query("SELECT current_schema()::text").await.unwrap().rows,
        vec![vec![Value::Text("enum_csv_shadow_c".into())]],
        "execution must use the changed search_path with a different same-named enum first"
    );
    connection.execute_in_transaction(&statements).await.unwrap();

    let target = connection
        .query(
            "SELECT id, state::text, pg_typeof(state)::text, \
             encode(convert_to(state::text, 'UTF8'), 'hex') \
             FROM enum_csv_shadow_b.target_rows ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        target.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("target-only".into()),
                Value::Text("enum_csv_shadow_b.state".into()),
                Value::Text("7461726765742d6f6e6c79".into()),
            ],
            vec![
                Value::Int(2),
                Value::Text("NULL".into()),
                Value::Text("enum_csv_shadow_b.state".into()),
                Value::Text("4e554c4c".into()),
            ],
            vec![
                Value::Int(3),
                Value::Text(String::new()),
                Value::Text("enum_csv_shadow_b.state".into()),
                Value::Text(String::new()),
            ],
            vec![
                Value::Int(4),
                Value::Null,
                Value::Text("enum_csv_shadow_b.state".into()),
                Value::Null,
            ],
            vec![
                Value::Int(99),
                Value::Text("ready".into()),
                Value::Text("enum_csv_shadow_b.state".into()),
                Value::Text("7265616479".into()),
            ],
        ]
    );
    let shadow = connection
        .query("SELECT count(*)::bigint FROM enum_csv_shadow_a.target_rows")
        .await
        .unwrap();
    assert_eq!(shadow.rows, vec![vec![Value::Int(0)]]);
}
