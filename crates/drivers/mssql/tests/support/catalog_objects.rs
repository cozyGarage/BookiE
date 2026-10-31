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
