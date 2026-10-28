use super::{connect, start_mariadb, start_mysql};
use tablepro_core::Connection;

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
    assert!(!tables.iter().any(|item| item.name == "listing_view"));
    assert!(views.iter().any(|item| item.name == "listing_view"));
    assert!(!views.iter().any(|item| item.name == "listing_items"));
}
