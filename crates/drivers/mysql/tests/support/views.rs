use super::{connect, start_mariadb, start_mysql};
use bookie_core::Connection;

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
