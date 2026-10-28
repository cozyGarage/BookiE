use tablepro_core::{DatabaseDriver, Value};
#[path = "../../../shared/exact_grid_original.rs"]
mod contract;

#[tokio::test]
#[ignore = "requires docker"]
async fn exact_grid_original_detects_concurrent_text_changes() {
    let (_fixture, options) = crate::shared_container::start_mysql().await;
    let connection = drivers_mysql::MysqlDriver.connect(options).await.unwrap();
    assert_mysql_collations(connection.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn exact_grid_original_detects_concurrent_mariadb_text_changes() {
    let (_fixture, options) = crate::shared_container::start_mariadb().await;
    let connection = drivers_mysql::MysqlDriver.connect(options).await.unwrap();
    assert_mysql_collations(connection.as_ref()).await;
}

async fn assert_mysql_collations(connection: &dyn tablepro_core::Connection) {
    for (kind, old, concurrent, search_matches) in [
        (
            "varchar(40) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci",
            "Ada",
            "ADA",
            Some(2),
        ),
        (
            "varchar(40) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci",
            "café",
            "cafe",
            Some(2),
        ),
        (
            "varchar(40) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci",
            "Ada",
            "Ada ",
            Some(2),
        ),
        (
            "varchar(40) CHARACTER SET latin1 COLLATE latin1_general_ci",
            "café",
            "cafÉ",
            Some(2),
        ),
    ] {
        connection
            .execute(&format!("CREATE TABLE exact_grid (id INT PRIMARY KEY, v {kind})"))
            .await
            .unwrap();
        contract::assert_conflict(
            connection,
            "mysql",
            Value::Text(old.into()),
            Value::Text(concurrent.into()),
            search_matches,
        )
        .await;
        connection.execute("DROP TABLE exact_grid").await.unwrap();
    }
    contract::assert_key_conflict(
        connection,
        "mysql",
        "varchar(40) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci",
    )
    .await;
    connection
        .execute("CREATE TABLE exact_grid (id INT PRIMARY KEY, v varbinary(40))")
        .await
        .unwrap();
    contract::assert_conflict(
        connection,
        "mysql",
        Value::Bytes(vec![65]),
        Value::Bytes(vec![65, 0]),
        None,
    )
    .await;
    connection.execute("DROP TABLE exact_grid").await.unwrap();
}
