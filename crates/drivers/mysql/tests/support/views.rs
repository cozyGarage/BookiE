use super::{connect, start_mariadb, start_mysql};
use bookie_core::{Connection, ViewEditability};

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_identifies_view_keys_and_read_only_columns() {
    let (_container, opts) = start_mysql().await;
    assert_view_editability(connect(opts).await.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_identifies_view_keys_and_read_only_columns() {
    let (_container, opts) = start_mariadb().await;
    assert_view_editability(connect(opts).await.as_ref()).await;
}

async fn assert_view_editability(conn: &dyn Connection) {
    for sql in [
        "CREATE TABLE editable_base (id INT NOT NULL, part INT NOT NULL, name VARCHAR(50), PRIMARY KEY (id, part))",
        "CREATE VIEW editable_full AS SELECT id AS item_id, part, name, UPPER(name) AS upper_name FROM editable_base",
        "CREATE VIEW editable_partial AS SELECT id, name FROM editable_base",
        "CREATE VIEW editable_spoof AS SELECT id + 0 AS id, part, name FROM editable_base",
        "CREATE VIEW editable_aggregate AS SELECT name, COUNT(*) AS total FROM editable_base GROUP BY name",
    ] {
        conn.execute(sql).await.unwrap();
    }
    assert_eq!(
        conn.fetch_view_editability(None, "editable_base").await.unwrap(),
        ViewEditability::NotApplicable
    );
    let ViewEditability::Editable {
        key_columns,
        read_only_columns,
        ..
    } = conn.fetch_view_editability(None, "editable_full").await.unwrap()
    else {
        panic!("a view exposing the complete primary key must be editable");
    };
    assert_eq!(key_columns, ["item_id", "part"]);
    assert_eq!(read_only_columns, ["upper_name"]);
    assert_eq!(
        conn.fetch_view_editability(None, "editable_partial").await.unwrap(),
        ViewEditability::KeyNotExposed
    );
    assert_eq!(
        conn.fetch_view_editability(None, "editable_spoof").await.unwrap(),
        ViewEditability::KeyNotExposed
    );
    assert_eq!(
        conn.fetch_view_editability(None, "editable_aggregate").await.unwrap(),
        ViewEditability::NotUpdatable
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn lists_tables_and_views_separately() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    assert_separate_views(conn.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_lists_tables_and_views_separately() {
    let (_container, opts) = start_mariadb().await;
    let conn = connect(opts).await;
    assert_separate_views(conn.as_ref()).await;
}

async fn assert_separate_views(conn: &dyn Connection) {
    conn.execute("CREATE TABLE listing_items (id INT)").await.unwrap();
    conn.execute("CREATE VIEW listing_view AS SELECT id FROM listing_items")
        .await
        .unwrap();
    let tables = conn.list_tables().await.unwrap();
    let views = conn.list_views().await.unwrap();
    assert!(tables.iter().any(|item| item.name == "listing_items"));
    assert!(tables.iter().any(|item| item.name == "listing_view"));
    assert!(views.iter().any(|item| item.name == "listing_view"));
    assert!(!views.iter().any(|item| item.name == "listing_items"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn lists_server_collations_with_their_character_sets() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    let collations = conn.list_collations().await.unwrap();
    assert!(collations.len() <= bookie_core::MAX_COLLATIONS);
    let bin = collations.iter().find(|item| item.name == "utf8mb4_bin").unwrap();
    assert_eq!(bin.charset, "utf8mb4");
    assert!(
        collations
            .iter()
            .any(|item| item.charset == "utf8mb4" && item.is_default)
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_picked_collation_is_applied_to_the_column() {
    use bookie_core::sql_ddl::{DraftColumn, TableState, diff_to_ops, materialize_ops};

    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE picked_collation (label varchar(40) COLLATE utf8mb4_general_ci NOT NULL)")
        .await
        .unwrap();
    let original = conn.fetch_columns(None, "picked_collation").await.unwrap();
    assert_eq!(original[0].collation.as_deref(), Some("utf8mb4_general_ci"));
    let listed = conn.list_collations().await.unwrap();
    assert!(listed.iter().any(|item| item.name == "utf8mb4_bin"));

    let mut draft: Vec<DraftColumn> = original.iter().cloned().map(DraftColumn::from_info).collect();
    draft[0].collation = Some("utf8mb4_bin".into());
    let ops = diff_to_ops(
        None,
        TableState::bare("picked_collation", &original),
        TableState::bare("picked_collation", &draft),
    );
    for statement in materialize_ops(&ops, "mysql").unwrap() {
        conn.execute(&statement).await.unwrap();
    }

    let applied = conn.fetch_columns(None, "picked_collation").await.unwrap();
    assert_eq!(applied[0].collation.as_deref(), Some("utf8mb4_bin"));
    assert!(!applied[0].nullable);
}
