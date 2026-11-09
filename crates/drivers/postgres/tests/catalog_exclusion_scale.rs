#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use bookie_core::{CatalogObjectKind, ConnectOptions, Connection, DatabaseDriver, TlsConfig};
use drivers_postgres::PgDriver;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

async fn measure_median(connection: &dyn Connection, excluded: &[String]) -> (Duration, usize) {
    let mut durations = Vec::with_capacity(3);
    let mut row_count = 0;
    for _ in 0..3 {
        let start = Instant::now();
        row_count = connection
            .list_tables_excluding(excluded)
            .await
            .expect("catalog query")
            .len();
        durations.push(start.elapsed());
    }
    durations.sort_unstable();
    (durations[1], row_count)
}

async fn measure_scoped_median(connection: &dyn Connection, included: &[String]) -> (Duration, usize) {
    let mut durations = Vec::with_capacity(3);
    let mut row_count = 0;
    for _ in 0..3 {
        let start = Instant::now();
        row_count = connection
            .list_tables_scoped(included)
            .await
            .expect("scoped catalog query")
            .len();
        durations.push(start.elapsed());
    }
    durations.sort_unstable();
    (durations[1], row_count)
}

async fn assert_exclusion_profile(
    connection: &dyn Connection,
    excluded: &[String],
    included: &[String],
) -> (Duration, Duration) {
    let (baseline, baseline_rows) = measure_scoped_median(connection, included).await;
    let (filtered, filtered_rows) = measure_median(connection, excluded).await;
    assert_eq!(baseline_rows, 500);
    assert_eq!(filtered_rows, baseline_rows);
    assert!(
        filtered <= baseline * 2 + Duration::from_millis(50),
        "large exclusion query took {filtered:?} against {baseline:?} same-result scoped baseline"
    );
    (baseline, filtered)
}

async fn assert_exclusion_edges(connection: &dyn Connection, excluded: &[String]) {
    let empty_rows = connection
        .list_tables_excluding(&[])
        .await
        .expect("empty exclusion")
        .len();
    let nonmatching = ["catalog_perf_absent".to_string()];
    let nonmatching_rows = connection
        .list_tables_excluding(&nonmatching)
        .await
        .expect("nonmatching exclusion")
        .len();
    let small_exclusion = ["catalog_perf_00001".to_string()];
    let small_rows = connection
        .list_tables_excluding(&small_exclusion)
        .await
        .expect("single-schema exclusion");
    let duplicated_exclusion = ["catalog_perf_00001".to_string(), "catalog_perf_00001".to_string()];
    let duplicate_rows = connection
        .list_tables_excluding(&duplicated_exclusion)
        .await
        .expect("duplicate-schema exclusion")
        .len();
    assert_eq!(empty_rows, 14_000);
    assert_eq!(nonmatching_rows, 14_000);
    assert_eq!(small_rows.len(), 13_998);
    assert_eq!(duplicate_rows, 13_998);
    assert_excluded_objects(connection, excluded).await;
}

async fn assert_excluded_objects(connection: &dyn Connection, excluded: &[String]) {
    let views = connection
        .list_views_excluding(excluded)
        .await
        .expect("large view exclusion");
    let types = connection
        .list_objects_excluding(CatalogObjectKind::Type, None, excluded)
        .await
        .expect("large type exclusion");
    assert!(
        views
            .iter()
            .any(|view| view.schema.as_deref() == Some("catalog_perf_09901"))
    );
    assert!(
        views
            .iter()
            .all(|view| view.schema.as_deref() != Some("catalog_perf_00001"))
    );
    assert!(
        types
            .iter()
            .any(|item| item.schema.as_deref() == Some("catalog_perf_09901"))
    );
    assert!(
        types
            .iter()
            .all(|item| item.schema.as_deref() != Some("catalog_perf_00001"))
    );
}

async fn connect(options: &ConnectOptions) -> Box<dyn Connection> {
    PgDriver
        .connect(options.clone())
        .await
        .expect("connect to PostgreSQL 18")
}

async fn seed_schemas(connection: &dyn Connection, start: usize, end: usize) {
    let sql = format!(
        "DO $$ DECLARE i integer; BEGIN \
         FOR i IN {start}..{end} LOOP \
           EXECUTE format('CREATE SCHEMA catalog_perf_%s', lpad(i::text, 5, '0')); \
           EXECUTE format('CREATE TABLE catalog_perf_%s.base (id integer)', lpad(i::text, 5, '0')); \
         END LOOP; END $$;"
    );
    connection.execute(&sql).await.expect("create schema batch");
}

async fn seed_extra_tables(connection: &dyn Connection, start: usize, end: usize) {
    let sql = format!(
        "DO $$ DECLARE i integer; BEGIN \
         FOR i IN {start}..{end} LOOP \
           EXECUTE format('CREATE TABLE catalog_perf_%s.extra_hidden (id integer)', lpad(i::text, 5, '0')); \
         END LOOP; END $$;"
    );
    connection.execute(&sql).await.expect("create hidden table batch");
}

async fn seed_extra_visible_tables(connection: &dyn Connection, start: usize, end: usize) {
    let sql = format!(
        "DO $$ DECLARE i integer; BEGIN \
         FOR i IN {start}..{end} LOOP \
           EXECUTE format('CREATE TABLE catalog_perf_%s.extra_visible_%s (id integer)', \
             lpad((9901 + (i % 106))::text, 5, '0'), i); \
         END LOOP; END $$;"
    );
    connection.execute(&sql).await.expect("create visible table batch");
}

async fn seed_catalog(connection: &dyn Connection) {
    for start in (1..=10006).step_by(100) {
        seed_schemas(connection, start, (start + 99).min(10006)).await;
    }
    for start in (1..=3600).step_by(100) {
        seed_extra_tables(connection, start, (start + 99).min(3600)).await;
    }
    for start in (1..=394).step_by(100) {
        seed_extra_visible_tables(connection, start, (start + 99).min(394)).await;
    }
    for statement in [
        "CREATE VIEW catalog_perf_00001.hidden_view AS SELECT 1 AS id",
        "CREATE VIEW catalog_perf_09901.visible_view AS SELECT 1 AS id",
        "CREATE TYPE catalog_perf_00001.hidden_kind AS ENUM ('hidden')",
        "CREATE TYPE catalog_perf_09901.visible_kind AS ENUM ('visible')",
    ] {
        connection
            .execute(statement)
            .await
            .expect("create view and type fixture");
    }
}

#[tokio::test]
async fn large_schema_exclusion_cost_stays_close_to_same_result_scoped_catalog_scan() {
    let image = Postgres::default()
        .with_tag("18-alpine")
        .with_startup_timeout(Duration::from_secs(180));
    let image = match std::env::var("BOOKIE_TEST_RUN_ID") {
        Ok(run_id) => image.with_label("com.bookie.test-run", run_id),
        Err(_) => image,
    };
    let container = image.start().await.expect("start PostgreSQL 18");
    let options = ConnectOptions {
        host: container.get_host().await.expect("container host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("PostgreSQL port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let connection = connect(&options).await;
    seed_catalog(connection.as_ref()).await;

    let excluded = (1..=9900)
        .map(|index| format!("catalog_perf_{index:05}"))
        .collect::<Vec<_>>();
    let included = (9901..=10006)
        .map(|index| format!("catalog_perf_{index:05}"))
        .collect::<Vec<_>>();
    assert_exclusion_profile(connection.as_ref(), &excluded, &included).await;
    assert_exclusion_edges(connection.as_ref(), &excluded).await;
    connection
        .execute("ALTER DATABASE postgres SET plan_cache_mode TO 'force_generic_plan'")
        .await
        .expect("select generic plan mode");
    connection.close().await.expect("close custom-plan pool");

    let connection = connect(&options).await;
    assert_exclusion_profile(connection.as_ref(), &excluded, &included).await;
}
