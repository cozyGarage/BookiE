#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;

#[tokio::test]
#[ignore = "requires docker"]
async fn connect_list_tables_pk_and_identity() {
    let (_c, opts) = start_mssql().await;
    let conn = connect(opts).await;

    conn.execute(
        "CREATE TABLE pk_demo (
            id int IDENTITY(1,1) PRIMARY KEY,
            name nvarchar(255) NOT NULL,
            note nvarchar(max) NULL
        )",
    )
    .await
    .unwrap();
    conn.execute("INSERT INTO pk_demo (name, note) VALUES (N'a', NULL), (N'b', N'second')")
        .await
        .unwrap();

    let tables = conn.list_tables().await.unwrap();
    assert!(tables.iter().any(|t| t.name == "pk_demo"));

    let cols = conn.fetch_columns(None, "pk_demo").await.unwrap();
    assert_eq!(cols.len(), 3);
    let id_col = cols.iter().find(|c| c.name == "id").unwrap();
    assert!(id_col.primary_key, "id must be detected as primary key");
    assert!(id_col.is_auto_increment, "IDENTITY must flag auto-increment");
    assert!(!id_col.nullable);
    let note_col = cols.iter().find(|c| c.name == "note").unwrap();
    assert!(!note_col.primary_key);
    assert!(note_col.nullable);

    let result = conn.fetch_rows(None, "pk_demo", 0, 100).await.unwrap();
    assert_eq!(result.rows.len(), 2);
    assert!(!result.truncated);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn catalog_objects_inspect_non_dbo_triggers_and_routines() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts).await;

    conn.execute(
        "IF NOT EXISTS (SELECT * FROM sys.schemas WHERE name = 'sales') \
         EXEC('CREATE SCHEMA sales'); \
         CREATE TABLE sales.orders (id INT PRIMARY KEY, amount INT); \
         EXEC('CREATE TRIGGER sales.tr_orders_audit ON sales.orders AFTER INSERT AS BEGIN SET NOCOUNT ON; END'); \
         EXEC('CREATE PROCEDURE sales.usp_add_order AS BEGIN SELECT 1; END'); \
         EXEC('CREATE SEQUENCE sales.seq_orders AS BIGINT START WITH 1'); \
         EXEC('CREATE TYPE sales.order_status FROM VARCHAR(20)'); \
         CREATE ROLE app_order_writer;",
    )
    .await
    .unwrap();

    let triggers = conn
        .list_objects(CatalogObjectKind::Trigger, Some("sales"))
        .await
        .unwrap();
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].schema.as_deref(), Some("sales"));
    assert_eq!(triggers[0].name, "tr_orders_audit");
    assert_eq!(triggers[0].detail.as_deref(), Some("on orders"));

    conn.execute("DISABLE TRIGGER sales.tr_orders_audit ON sales.orders")
        .await
        .unwrap();
    let disabled = conn
        .list_objects(CatalogObjectKind::Trigger, Some("sales"))
        .await
        .unwrap();
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].detail.as_deref(), Some("on orders (disabled)"));

    let routines = conn
        .list_objects(CatalogObjectKind::Routine, Some("sales"))
        .await
        .unwrap();
    assert_eq!(routines.len(), 1);
    assert_eq!(routines[0].schema.as_deref(), Some("sales"));
    assert_eq!(routines[0].name, "usp_add_order");
    assert_eq!(routines[0].detail.as_deref(), Some("procedure"));

    let sequences = conn
        .list_objects(CatalogObjectKind::Sequence, Some("sales"))
        .await
        .unwrap();
    assert_eq!(sequences.len(), 1);
    assert_eq!(sequences[0].schema.as_deref(), Some("sales"));
    assert_eq!(sequences[0].name, "seq_orders");

    let types = conn.list_objects(CatalogObjectKind::Type, Some("sales")).await.unwrap();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].schema.as_deref(), Some("sales"));
    assert_eq!(types[0].name, "order_status");
    assert_eq!(types[0].detail.as_deref(), Some("alias type"));

    let roles = conn.list_objects(CatalogObjectKind::Role, None).await.unwrap();
    let app_role = roles.iter().find(|r| r.name == "app_order_writer").unwrap();
    assert_eq!(app_role.detail.as_deref(), Some("database role"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn batch_query_supports_temp_tables_without_params() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts).await;

    let res = conn
        .query(
            "CREATE TABLE #temp_items (id INT, label VARCHAR(20)); \
             INSERT INTO #temp_items VALUES (10, 'alpha'), (20, 'beta'); \
             SELECT id, label FROM #temp_items ORDER BY id;",
        )
        .await
        .unwrap();

    assert_eq!(res.rows.len(), 2);
    assert_eq!(res.rows[0], vec![Value::Int(10), Value::Text("alpha".into())]);
    assert_eq!(res.rows[1], vec![Value::Int(20), Value::Text("beta".into())]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn list_views_includes_non_dbo_views_and_excludes_tables() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts).await;

    conn.execute("EXEC('CREATE SCHEMA sales')").await.unwrap();
    conn.execute("CREATE TABLE sales.orders (id INT PRIMARY KEY)")
        .await
        .unwrap();
    conn.execute("EXEC('CREATE VIEW sales.v_orders AS SELECT id FROM sales.orders')")
        .await
        .unwrap();
    conn.execute("EXEC('CREATE VIEW dbo.v_plain AS SELECT 1 AS one')")
        .await
        .unwrap();

    let views = conn.list_views().await.unwrap();
    let names: Vec<(Option<&str>, &str)> = views.iter().map(|v| (v.schema.as_deref(), v.name.as_str())).collect();
    assert!(names.contains(&(Some("sales"), "v_orders")), "{names:?}");
    assert!(names.contains(&(Some("dbo"), "v_plain")), "{names:?}");
    assert!(!names.iter().any(|(_, name)| *name == "orders"), "{names:?}");

    let tables = conn.list_tables().await.unwrap();
    assert!(!tables.iter().any(|t| t.name == "v_orders"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn lists_server_collations_with_one_default() {
    let (_c, opts) = start_mssql().await;
    let conn = connect(opts).await;
    let collations = conn.list_collations().await.unwrap();
    assert!(collations.len() <= bookie_core::MAX_COLLATIONS);
    let default = collations.iter().filter(|item| item.is_default).collect::<Vec<_>>();
    assert_eq!(default.len(), 1, "{default:?}");
    assert!(collations[0].is_default);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn routine_definitions_are_found_by_name_and_hidden_from_a_role_without_view_definition() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts.clone()).await;
    conn.execute(
        "EXEC('CREATE SCHEMA rd_sales'); \
         EXEC('CREATE PROCEDURE rd_sales.open_proc AS BEGIN SELECT 1; END'); \
         EXEC('CREATE PROCEDURE rd_sales.exec_only_proc AS BEGIN SELECT 2; END'); \
         EXEC('CREATE PROCEDURE rd_sales.hidden_proc AS BEGIN SELECT 3; END'); \
         CREATE LOGIN rd_login WITH PASSWORD = 'Restricted#2024pw', CHECK_POLICY = OFF; \
         CREATE USER rd_user FOR LOGIN rd_login; \
         GRANT EXECUTE ON rd_sales.open_proc TO rd_user; \
         GRANT VIEW DEFINITION ON rd_sales.open_proc TO rd_user; \
         GRANT EXECUTE ON rd_sales.exec_only_proc TO rd_user;",
    )
    .await
    .unwrap();
    let routine = |name: &str| bookie_core::RoutineRef {
        schema: "rd_sales".into(),
        name: name.into(),
        identity_arguments: None,
    };

    let found = match conn.routine_definition(&routine("open_proc")).await.unwrap() {
        bookie_core::RoutineLookup::Found(definition) => definition,
        other => panic!("{other:?}"),
    };
    assert_eq!(found.kind, "procedure");
    assert!(found.definition.contains("SELECT 1"));
    assert!(!found.truncated);
    assert_eq!(
        conn.routine_definition(&routine("missing")).await.unwrap(),
        bookie_core::RoutineLookup::NotFound
    );

    let restricted = connect(ConnectOptions {
        username: "rd_login".into(),
        password: secrecy::SecretString::new("Restricted#2024pw".to_string().into()),
        ..opts
    })
    .await;
    assert!(matches!(
        restricted.routine_definition(&routine("open_proc")).await.unwrap(),
        bookie_core::RoutineLookup::Found(_)
    ));
    for hidden in ["exec_only_proc", "hidden_proc"] {
        assert_eq!(
            restricted.routine_definition(&routine(hidden)).await.unwrap(),
            bookie_core::RoutineLookup::NotFound,
            "{hidden}"
        );
    }
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    assert!(matches!(
        conn.routine_definition_controlled(&routine("open_proc"), &control)
            .await
            .unwrap(),
        bookie_core::RoutineLookup::Found(_)
    ));
}
