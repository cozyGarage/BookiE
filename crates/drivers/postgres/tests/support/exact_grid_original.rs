use tablepro_core::{DatabaseDriver, Value};
#[path = "../../../shared/exact_grid_original.rs"]
mod contract;

#[tokio::test]
#[ignore = "requires docker"]
async fn exact_grid_original_detects_concurrent_text_changes() {
    let (_fixture, options) = super::shared_container::start_pg().await;
    let connection = drivers_postgres::PgDriver.connect(options).await.unwrap();
    connection.execute("CREATE EXTENSION citext").await.unwrap();
    connection
        .execute("CREATE COLLATION exact_ci (provider = icu, locale = 'und-u-ks-level1', deterministic = false)")
        .await
        .unwrap();
    for (kind, old, concurrent, search_matches) in [
        ("citext", "Ada", "ADA", None),
        ("citext[]", "{Ada}", "{ADA}", None),
        ("text[] COLLATE exact_ci", "{café}", "{cafe}", Some(2)),
        ("text COLLATE exact_ci", "café", "cafe", Some(2)),
        ("text COLLATE exact_ci", "Ada", "ADA", Some(2)),
        ("varchar(40)", "Ada", "Ada ", Some(1)),
    ] {
        connection
            .execute(&format!("CREATE TABLE exact_grid (id INT PRIMARY KEY, v {kind})"))
            .await
            .unwrap();
        contract::assert_conflict(
            connection.as_ref(),
            "postgres",
            Value::Text(old.into()),
            Value::Text(concurrent.into()),
            search_matches,
        )
        .await;
        connection.execute("DROP TABLE exact_grid").await.unwrap();
    }
    contract::assert_key_conflict(connection.as_ref(), "postgres", "text COLLATE exact_ci").await;
    connection
        .execute("CREATE TABLE exact_grid (id INT PRIMARY KEY, v bytea)")
        .await
        .unwrap();
    contract::assert_conflict(
        connection.as_ref(),
        "postgres",
        Value::Bytes(vec![65]),
        Value::Bytes(vec![65, 0]),
        None,
    )
    .await;
    connection.execute("DROP TABLE exact_grid").await.unwrap();

    connection
        .execute("CREATE TABLE exact_grid (id INT PRIMARY KEY, v json)")
        .await
        .unwrap();
    contract::assert_conflict(
        connection.as_ref(),
        "postgres",
        Value::JsonText("{\"a\":1,\"a\":2}".into()),
        Value::JsonText("{\"a\":2}".into()),
        None,
    )
    .await;
    connection.execute("DROP TABLE exact_grid").await.unwrap();
}
