#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bookie_core::{
    ConnectOptions, Connection, DatabaseDriver, HealthReport, OperationControl, QueryResult, TlsConfig, Value,
    hidden_schemas_param,
};
use drivers_postgres::PgDriver;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::core::IntoContainerPort;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const SEED: &[&str] = &[
    "CREATE SCHEMA health_a",
    "CREATE SCHEMA health_hidden",
    "CREATE TABLE health_a.bloaty (id int PRIMARY KEY, a int, b int, payload text) WITH (autovacuum_enabled = false)",
    "INSERT INTO health_a.bloaty SELECT g, g, g, repeat('x', 200) FROM generate_series(1, 20000) g",
    "DELETE FROM health_a.bloaty WHERE id % 2 = 0",
    "CREATE INDEX unused_idx ON health_a.bloaty (payload)",
    "CREATE UNIQUE INDEX unique_unused_idx ON health_a.bloaty (a)",
    "CREATE INDEX dup_short ON health_a.bloaty (b)",
    "CREATE INDEX dup_long ON health_a.bloaty (b, a)",
    "CREATE INDEX twin_one ON health_a.bloaty (payload, a)",
    "CREATE INDEX twin_two ON health_a.bloaty (payload, a)",
    "CREATE COLLATION health_a.other_c (provider = libc, locale = 'C')",
    "CREATE TABLE health_a.collated (v text)",
    "CREATE INDEX collate_c ON health_a.collated (v COLLATE \"C\")",
    "CREATE INDEX collate_other ON health_a.collated (v COLLATE health_a.other_c)",
    "CREATE TABLE health_a.scanned (id int, v int) WITH (autovacuum_enabled = false)",
    "INSERT INTO health_a.scanned SELECT g, g FROM generate_series(1, 12000) g",
    "CREATE TABLE health_a.tiny (id int) WITH (autovacuum_enabled = false)",
    "INSERT INTO health_a.tiny SELECT g FROM generate_series(1, 5) g",
    "CREATE TABLE health_hidden.secret (id int) WITH (autovacuum_enabled = false)",
    "INSERT INTO health_hidden.secret SELECT g FROM generate_series(1, 2000) g",
    "DELETE FROM health_hidden.secret WHERE id % 2 = 0",
    "CREATE ROLE health_blind LOGIN PASSWORD 'blind'",
    "GRANT USAGE ON SCHEMA health_a TO health_blind",
];

fn control() -> OperationControl {
    OperationControl::with_timeout(Duration::from_secs(20))
}

async fn run(connection: &dyn Connection, report: HealthReport, hidden: &[String]) -> QueryResult {
    connection
        .query_params_controlled(&report.sql(), &[hidden_schemas_param(hidden)], &control())
        .await
        .unwrap_or_else(|error| panic!("{report:?} failed: {error}"))
}

fn names(result: &QueryResult, column: &str) -> Vec<String> {
    let index = result.columns.iter().position(|c| c.name == column).unwrap();
    result
        .rows
        .iter()
        .filter_map(|row| match &row[index] {
            Value::Text(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

async fn eventually(
    connection: &dyn Connection,
    report: HealthReport,
    ready: impl Fn(&QueryResult) -> bool,
) -> QueryResult {
    for _ in 0..50 {
        let result = run(connection, report, &[]).await;
        if ready(&result) {
            return result;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    panic!("{report:?} never reported the expected rows");
}

async fn start_postgres_16() -> ContainerAsync<Postgres> {
    Postgres::default()
        .with_tag("16-alpine")
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("start postgres")
}

async fn start_postgres_18() -> ContainerAsync<Postgres> {
    Postgres::default()
        .with_tag("18-alpine")
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("start postgres")
}

async fn exercise(container: ContainerAsync<Postgres>) {
    let options = ConnectOptions {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(5432.tcp()).await.unwrap(),
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let admin = PgDriver.connect(options.clone()).await.unwrap();
    for statement in SEED {
        admin.execute(statement).await.unwrap();
    }
    for _ in 0..4 {
        for table in ["scanned", "tiny"] {
            admin
                .query(&format!("SELECT count(*) FROM health_a.{table} WHERE id = -1"))
                .await
                .unwrap();
        }
    }

    let vacuum = eventually(admin.as_ref(), HealthReport::TableVacuum, |r| {
        names(r, "table_name").contains(&"bloaty".to_owned())
    })
    .await;
    assert!(
        names(&vacuum, "schema_name")
            .iter()
            .all(|schema| schema != "pg_catalog")
    );

    let bloat = eventually(admin.as_ref(), HealthReport::EstimatedBloat, |r| {
        names(r, "table_name").contains(&"bloaty".to_owned())
    })
    .await;
    assert!(names(&bloat, "estimated_bloat").len() == bloat.rows.iter().count());

    let unused = eventually(admin.as_ref(), HealthReport::UnusedIndexes, |r| {
        names(r, "index_name").contains(&"unused_idx".to_owned())
    })
    .await;
    let unused_names = names(&unused, "index_name");
    assert!(!unused_names.contains(&"unique_unused_idx".to_owned()));
    assert!(!unused_names.contains(&"bloaty_pkey".to_owned()));

    let duplicates = run(admin.as_ref(), HealthReport::DuplicateIndexes, &[]).await;
    let duplicate_names = names(&duplicates, "index_name");
    assert!(duplicate_names.contains(&"dup_short".to_owned()), "{duplicate_names:?}");
    assert!(!duplicate_names.contains(&"dup_long".to_owned()));
    assert!(
        !duplicate_names
            .iter()
            .any(|name| name == "collate_c" || name == "collate_other"),
        "indexes with different collations are not duplicates: {duplicate_names:?}"
    );
    assert!(!duplicate_names.contains(&"unique_unused_idx".to_owned()));
    let twins = duplicate_names.iter().filter(|name| name.starts_with("twin_")).count();
    assert_eq!(twins, 1, "{duplicate_names:?}");

    let scans = eventually(admin.as_ref(), HealthReport::SeqScanHeavy, |r| {
        names(r, "table_name").contains(&"scanned".to_owned())
    })
    .await;
    assert!(!names(&scans, "table_name").contains(&"tiny".to_owned()));

    let hidden = vec!["health_hidden".to_owned(), "health_a".to_owned()];
    for report in HealthReport::ALL {
        let result = run(admin.as_ref(), report, &hidden).await;
        let schemas = names(&result, "schema_name");
        assert!(
            schemas
                .iter()
                .all(|schema| schema != "health_hidden" && schema != "health_a"),
            "{report:?} reported a hidden schema"
        );
        assert!(result.rows.iter().count() <= bookie_core::HEALTH_ROW_LIMIT);
    }

    let blind = PgDriver
        .connect(ConnectOptions {
            username: "health_blind".into(),
            password: secrecy::SecretString::new("blind".to_string().into()),
            ..options.clone()
        })
        .await
        .unwrap();
    for report in HealthReport::ALL {
        let result = run(blind.as_ref(), report, &[]).await;
        assert!(
            names(&result, "schema_name").iter().all(|schema| schema != "health_a"),
            "{report:?} reported objects the role cannot see"
        );
    }
    admin.close().await.unwrap();
    blind.close().await.unwrap();
}

#[tokio::test]
async fn every_health_report_runs_and_decodes_on_postgres_16() {
    exercise(start_postgres_16().await).await;
}

#[tokio::test]
async fn every_health_report_runs_and_decodes_on_postgres_18() {
    exercise(start_postgres_18().await).await;
}
