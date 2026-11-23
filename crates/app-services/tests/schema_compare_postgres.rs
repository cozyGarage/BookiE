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
async fn the_generated_script_applies_adds_and_alters_but_only_lists_drops() {
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
    assert!(before.script.contains("-- DROP TABLE \"current_side\".\"legacy\";"));
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
        after.added.is_empty() && after.altered.is_empty() && after.dropped == ["legacy"],
        "the script should apply adds and alters while leaving listed drops untouched: {after:?}\n--- script ---\n{}",
        before.script
    );
    connection.close().await.expect("close");
}

#[tokio::test]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test schema_compare_postgres -- --include-ignored"]
async fn schema_diagram_reads_postgres_metadata_in_one_round_trip_and_matches_table_reads() {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .with_cmd([
            "postgres",
            "-c",
            "shared_preload_libraries=pg_stat_statements",
            "-c",
            "pg_stat_statements.track=all",
        ])
        .start()
        .await
        .expect("start postgres with statement statistics");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: SecretString::from("postgres"),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let admin = PgDriver.connect(options.clone()).await.expect("connect admin");
    admin
        .execute("CREATE EXTENSION pg_stat_statements")
        .await
        .expect("enable statement statistics");
    for statement in [
        "CREATE SCHEMA bulk_diagram",
        "CREATE TYPE bulk_diagram.person_state AS ENUM ('active', 'paused')",
        "CREATE DOMAIN bulk_diagram.email_address AS text CHECK (VALUE <> '')",
        "CREATE TABLE bulk_diagram.people (id int GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY, name text DEFAULT 'anonymous', state bulk_diagram.person_state, addresses bulk_diagram.email_address[])",
        "COMMENT ON COLUMN bulk_diagram.people.name IS 'display name'",
        "CREATE TABLE bulk_diagram.orders (id int PRIMARY KEY, person_id int REFERENCES bulk_diagram.people(id))",
        "CREATE TABLE bulk_diagram.lines (id int PRIMARY KEY, order_id int REFERENCES bulk_diagram.orders(id))",
        "CREATE TABLE bulk_diagram.pairs (left_id int, right_id int, PRIMARY KEY (left_id, right_id))",
        "CREATE TABLE bulk_diagram.pair_children (left_id int, right_id int, CONSTRAINT pair_fk FOREIGN KEY (left_id, right_id) REFERENCES bulk_diagram.pairs(left_id, right_id) ON UPDATE SET NULL ON DELETE CASCADE)",
    ] {
        admin.execute(statement).await.expect(statement);
    }
    let app = PgDriver
        .connect(ConnectOptions {
            application_name: Some("bookie_er_diagram_batch".into()),
            ..options
        })
        .await
        .expect("connect app");
    let tables: Vec<_> = app
        .list_tables()
        .await
        .expect("list schema tables")
        .into_iter()
        .filter(|table| table.schema.as_deref() == Some("bulk_diagram"))
        .collect();
    admin
        .query("SELECT pg_stat_statements_reset()")
        .await
        .expect("reset statement statistics");
    let control = OperationControl::new(CancellationToken::new(), None);
    let batched = read_schema(
        app.as_ref(),
        Some("bulk_diagram"),
        &tables,
        PgDriver.capabilities(),
        &control,
        |_, _| {},
    )
    .await
    .expect("read schema diagram");
    assert!(batched.skipped.is_empty());
    assert_eq!(
        admin
            .query(
                "SELECT COALESCE(SUM(calls), 0)::bigint FROM pg_stat_statements WHERE query LIKE 'WITH requested AS (%'"
            )
            .await
            .expect("count schema metadata statements")
            .rows,
        vec![vec![bookie_core::Value::Int(1)]]
    );

    let mut per_table = Vec::new();
    for table in &tables {
        per_table.push((
            table.name.clone(),
            app.fetch_columns_controlled(table.schema.as_deref(), &table.name, &control)
                .await
                .expect("read columns"),
            app.fetch_foreign_keys_controlled(table.schema.as_deref(), &table.name, &control)
                .await
                .expect("read foreign keys"),
        ));
    }
    let batched_values = batched
        .reads
        .into_iter()
        .map(|table| (table.name, table.columns, table.foreign_keys))
        .collect::<Vec<_>>();
    assert_eq!(batched_values, per_table);
}
