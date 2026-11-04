#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_app_services::er_diagram::read::read_schema;
use bookie_app_services::schema_compare::compare_schemas;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, OperationControl, TlsConfig};
use drivers_postgres::PgDriver;
use secrecy::SecretString;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_util::sync::CancellationToken;

const SEED: &[&str] = &[
    "CREATE SCHEMA current_side",
    "CREATE SCHEMA desired_side",
    "CREATE TABLE current_side.people (id int PRIMARY KEY, name text, age int NOT NULL)",
    "CREATE TABLE current_side.legacy (id int PRIMARY KEY)",
    "CREATE TABLE current_side.same (id int PRIMARY KEY, label text)",
    "CREATE TABLE desired_side.people (id int PRIMARY KEY, name varchar(40), age int, email text)",
    "CREATE TABLE desired_side.same (id int PRIMARY KEY, label text)",
    "CREATE TABLE desired_side.orders (id int PRIMARY KEY, person int REFERENCES desired_side.people (id))",
];

async fn read(connection: &dyn Connection, schema: &str) -> Vec<bookie_app_services::er_diagram::TableRead> {
    let control = OperationControl::new(CancellationToken::new(), None);
    let tables: Vec<_> = connection
        .list_tables()
        .await
        .expect("list tables")
        .into_iter()
        .filter(|table| table.schema.as_deref() == Some(schema))
        .collect();
    let read = read_schema(
        connection,
        Some(schema),
        &tables,
        PgDriver.capabilities(),
        &control,
        |_, _| {},
    )
    .await
    .expect("read schema");
    assert!(!read.truncated() && read.skipped.is_empty());
    read.reads
}

async fn diff(connection: &dyn Connection) -> bookie_app_services::schema_compare::SchemaCompare {
    let current = read(connection, "current_side").await;
    let desired = read(connection, "desired_side").await;
    compare_schemas(
        "postgres",
        Some("current_side"),
        &current,
        Some("desired_side"),
        &desired,
    )
    .expect("compare")
}

#[tokio::test]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test schema_compare_postgres -- --include-ignored"]
async fn the_generated_script_makes_the_current_schema_match_the_desired_one_on_a_real_server() {
    let container = Postgres::default()
        .with_tag("18-alpine")
        .start()
        .await
        .expect("start postgres container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: SecretString::from("postgres"),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let connection = PgDriver.connect(options).await.expect("connect");
    for statement in SEED {
        connection.execute(statement).await.expect(statement);
    }

    let before = diff(connection.as_ref()).await;
    assert_eq!(before.added, ["orders"]);
    assert_eq!(before.dropped, ["legacy"]);
    assert_eq!(
        before
            .altered
            .iter()
            .map(|change| change.table.as_str())
            .collect::<Vec<_>>(),
        ["people"]
    );

    for statement in bookie_core::sql_lex::split_statements(&before.script, "postgres") {
        if let Err(error) = connection.execute(&statement).await {
            panic!(
                "running {statement:?} failed: {error:?}\n--- script ---\n{}",
                before.script
            );
        }
    }

    let after = diff(connection.as_ref()).await;
    assert!(
        after.is_identical(),
        "the script left differences: {after:?}\n--- script ---\n{}",
        before.script
    );
    connection.close().await.expect("close");
}
