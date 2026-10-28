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
    connection
        .execute("CREATE DOMAIN exact_character AS bpchar")
        .await
        .unwrap();
    connection
        .execute("CREATE DOMAIN exact_citext AS citext")
        .await
        .unwrap();
    for (kind, old, concurrent, search_matches) in [
        ("exact_citext", "Ada", "ADA", None),
        ("bpchar", "Ada", "Ada ", None),
        ("exact_character", "Ada", "Ada ", None),
        ("citext", "Ada", "ADA", None),
        ("citext[]", "{\"Ada\"}", "{\"ADA\"}", None),
        ("text[] COLLATE exact_ci", "{\"café\"}", "{\"cafe\"}", Some(2)),
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
    contract::assert_key_conflict(connection.as_ref(), "postgres", "bpchar").await;
    assert_empty_text_is_distinct_from_null(connection.as_ref()).await;
    assert_shadowed_c_collation_is_exact(connection.as_ref()).await;
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

async fn assert_empty_text_is_distinct_from_null(connection: &dyn tablepro_core::Connection) {
    connection
        .execute("CREATE TABLE exact_grid (id INT PRIMARY KEY, v text)")
        .await
        .unwrap();
    contract::assert_conflict(connection, "postgres", Value::Text(String::new()), Value::Null, None).await;
    connection.execute("DROP TABLE exact_grid").await.unwrap();
}

async fn assert_shadowed_c_collation_is_exact(connection: &dyn tablepro_core::Connection) {
    use tablepro_app_services::change_tracker::{RowKey, TabChangeTracker};
    use tablepro_core::DriverError;

    connection.execute("CREATE SCHEMA shadow").await.unwrap();
    connection
        .execute("CREATE COLLATION shadow.\"C\" (provider = icu, locale = 'und-u-ks-level1', deterministic = false)")
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE public.exact_shadow (id INT PRIMARY KEY, v text)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO public.exact_shadow VALUES (1, 'ADA'), (2, 'Zoe')")
        .await
        .unwrap();
    let columns = connection.fetch_columns(Some("public"), "exact_shadow").await.unwrap();
    let mut tracker = TabChangeTracker::new();
    tracker.track_cell_edit(
        RowKey::from_pk_values(&[Value::Int(1)]).unwrap(),
        1,
        Value::Text("Ada".into()),
        Value::Text("saved".into()),
    );
    let mut statements = tracker
        .materialize("postgres", Some("public"), "exact_shadow", &columns)
        .unwrap()
        .0;
    statements.insert(0, ("SET LOCAL search_path = shadow, public, pg_catalog".into(), vec![]));
    let result = connection.execute_in_transaction_checked(&statements, &[1]).await;
    assert!(
        matches!(result, Err(DriverError::Transaction { source, .. }) if matches!(*source, DriverError::ConcurrentModification))
    );
    assert_eq!(
        connection
            .query("SELECT v FROM public.exact_shadow ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Text("ADA".into())], vec![Value::Text("Zoe".into())]]
    );
}
