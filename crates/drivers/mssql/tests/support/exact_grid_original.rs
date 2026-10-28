use tablepro_core::{DatabaseDriver, Value};
#[path = "../../../shared/exact_grid_original.rs"]
mod contract;

#[tokio::test]
#[ignore = "requires docker"]
async fn exact_grid_original_detects_concurrent_text_changes() {
    let (_fixture, options) = crate::shared_container::start_mssql().await;
    let connection = drivers_mssql::MssqlDriver.connect(options).await.unwrap();
    for (kind, old, concurrent, search_matches) in [
        ("nvarchar(40) COLLATE Latin1_General_CI_AI", "Ada", "ADA", Some(2)),
        ("nvarchar(40) COLLATE Latin1_General_CI_AI", "café", "cafe", Some(2)),
        ("nvarchar(40) COLLATE Latin1_General_CI_AI", "Ada", "Ada ", Some(2)),
        ("varchar(40) COLLATE Latin1_General_CI_AI", "café", "cafÉ", Some(2)),
    ] {
        connection
            .execute(&format!("CREATE TABLE exact_grid (id INT PRIMARY KEY, v {kind})"))
            .await
            .unwrap();
        contract::assert_conflict(
            connection.as_ref(),
            "mssql",
            Value::Text(old.into()),
            Value::Text(concurrent.into()),
            search_matches,
        )
        .await;
        connection.execute("DROP TABLE exact_grid").await.unwrap();
    }
    contract::assert_key_conflict(
        connection.as_ref(),
        "mssql",
        "nvarchar(40) COLLATE Latin1_General_CI_AI",
    )
    .await;
    connection
        .execute("CREATE TABLE exact_grid (id INT PRIMARY KEY, v varbinary(40))")
        .await
        .unwrap();
    contract::assert_conflict(
        connection.as_ref(),
        "mssql",
        Value::Bytes(vec![65]),
        Value::Bytes(vec![65, 0]),
        None,
    )
    .await;
    connection.execute("DROP TABLE exact_grid").await.unwrap();
}
