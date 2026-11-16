#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{CatalogObjectKind, ConnectOptions, Connection, DatabaseDriver, TlsConfig};
use drivers_postgres::PgDriver;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres as PostgresImage;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const FIXTURE: &[&str] = &[
    "CREATE SCHEMA shop",
    "CREATE TYPE shop.mood AS ENUM ('happy', 'sad')",
    "CREATE DOMAIN shop.qty AS integer CHECK (VALUE >= 0)",
    "CREATE TABLE shop.parents (a int, b int, PRIMARY KEY (a, b))",
    "CREATE TABLE shop.orders (id int PRIMARY KEY, a int, b int, qty shop.qty, mood shop.mood, \
     FOREIGN KEY (a, b) REFERENCES shop.parents (a, b))",
    "CREATE UNIQUE INDEX orders_qty_idx ON shop.orders (qty, (id + 1))",
    "CREATE VIEW shop.orders_view AS SELECT id, qty FROM shop.orders",
];

const SHADOW: &[&str] = &[
    "CREATE SCHEMA shadow",
    "CREATE TABLE shadow.pg_collation (collname name, collencoding int)",
    "CREATE TABLE shadow.pg_database (datname name)",
    "CREATE FUNCTION shadow.pg_encoding_to_char(int) RETURNS name LANGUAGE sql AS $$ SELECT 'SHADOW'::name $$",
    "CREATE FUNCTION shadow.current_database() RETURNS name LANGUAGE sql AS $$ SELECT 'shadow'::name $$",
    "CREATE FUNCTION shadow.has_database_privilege(name, text) RETURNS boolean LANGUAGE sql AS $$ SELECT false $$",
    "CREATE FUNCTION shadow.unnest(anyarray) RETURNS SETOF anyelement LANGUAGE sql AS $$ SELECT NULL::int2 WHERE false $$",
    "CREATE FUNCTION shadow.string_step(text, text, text) RETURNS text LANGUAGE sql AS $$ SELECT 'SHADOW' $$",
    "CREATE AGGREGATE shadow.string_agg(text, text) (SFUNC = shadow.string_step, STYPE = text)",
];

const SHADOW_ARRAY_AGG: &[&str] = &[
    "CREATE FUNCTION shadow.array_step(anyarray, anynonarray) RETURNS anyarray LANGUAGE sql AS $$ SELECT $1 $$",
    "CREATE AGGREGATE shadow.array_agg(anynonarray) (SFUNC = shadow.array_step, STYPE = anyarray)",
];

const FOREIGN_KEY_FACTS: [usize; 2] = [7, 8];

fn options(host: &str, port: u16, database: &str) -> ConnectOptions {
    ConnectOptions {
        host: host.into(),
        port,
        database: database.into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    }
}

async fn fingerprint(connection: &dyn Connection) -> Vec<String> {
    let mut facts = Vec::new();
    facts.push(format!("{:?}", connection.list_collations().await));
    facts.push(format!("{:?}", connection.list_databases().await));
    for kind in [
        CatalogObjectKind::Grant,
        CatalogObjectKind::Routine,
        CatalogObjectKind::Extension,
    ] {
        let listed = connection.list_objects(kind, Some("shop")).await;
        facts.push(format!("{listed:?}"));
    }
    facts.push(format!("{:?}", connection.fetch_columns(Some("shop"), "orders").await));
    facts.push(format!("{:?}", connection.fetch_indexes(Some("shop"), "orders").await));
    facts.push(format!(
        "{:?}",
        connection.fetch_foreign_keys(Some("shop"), "orders").await
    ));
    facts.push(format!(
        "{:?}",
        connection.fetch_inbound_foreign_keys(Some("shop"), "parents").await
    ));
    facts.push(format!(
        "{:?}",
        connection.fetch_view_editability(Some("shop"), "orders_view").await
    ));
    let queried = connection
        .query("SELECT id, qty, mood FROM shop.orders")
        .await
        .map(|result| format!("{:?}", result.columns));
    facts.push(format!("{queried:?}"));
    facts
}

async fn fingerprint_foreign_keys(connection: &dyn Connection) -> Vec<String> {
    vec![
        format!("{:?}", connection.fetch_foreign_keys(Some("shop"), "orders").await),
        format!(
            "{:?}",
            connection.fetch_inbound_foreign_keys(Some("shop"), "parents").await
        ),
    ]
}

async fn search_path(connection: &dyn Connection) -> String {
    format!("{:?}", connection.query("SHOW search_path").await.unwrap().rows)
}

#[tokio::test]
#[ignore = "requires docker"]
async fn catalog_queries_ignore_objects_that_shadow_catalog_names_on_the_search_path() {
    let container = PostgresImage::default().with_tag("16-alpine").start().await.unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin = PgDriver.connect(options(&host, port, "postgres")).await.unwrap();
    admin.execute("CREATE DATABASE shadow_probe").await.unwrap();
    let probe = PgDriver.connect(options(&host, port, "shadow_probe")).await.unwrap();
    for statement in FIXTURE {
        probe.execute(statement).await.expect(statement);
    }
    let baseline = fingerprint(probe.as_ref()).await;
    assert!(
        baseline.iter().all(|fact| !fact.contains("SHADOW")),
        "the baseline has no shadow objects: {baseline:?}"
    );

    for statement in SHADOW {
        probe.execute(statement).await.expect(statement);
    }
    admin
        .execute("ALTER DATABASE shadow_probe SET search_path = shadow, pg_catalog")
        .await
        .unwrap();
    let shadowed = PgDriver.connect(options(&host, port, "shadow_probe")).await.unwrap();
    let path_before = search_path(shadowed.as_ref()).await;
    assert!(path_before.contains("shadow, pg_catalog"), "{path_before}");

    let under_shadow = fingerprint(shadowed.as_ref()).await;

    for (index, (shadowed_fact, base_fact)) in under_shadow.iter().zip(&baseline).enumerate() {
        assert_eq!(shadowed_fact, base_fact, "fact {index}");
    }
    assert_eq!(search_path(shadowed.as_ref()).await, path_before);

    for statement in SHADOW_ARRAY_AGG {
        probe.execute(statement).await.expect(statement);
    }
    let with_aggregate = fingerprint_foreign_keys(shadowed.as_ref()).await;
    for (position, index) in FOREIGN_KEY_FACTS.into_iter().enumerate() {
        assert_eq!(with_aggregate[position], baseline[index], "foreign key fact {index}");
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn sqlx_type_resolution_ignores_a_shadowed_array_agg() {
    let container = PostgresImage::default().with_tag("16-alpine").start().await.unwrap();
    let options = options(
        &container.get_host().await.unwrap().to_string(),
        container.get_host_port_ipv4(5432).await.unwrap(),
        "postgres",
    );
    let setup = PgDriver.connect(options.clone()).await.unwrap();
    setup.execute("CREATE SCHEMA shadow").await.unwrap();
    setup.execute("CREATE SCHEMA shop").await.unwrap();
    setup
        .execute(
            "CREATE FUNCTION shadow.to_regtype(pg_catalog.text) RETURNS pg_catalog.regtype \
             LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'shadow type lookup invoked'; END $$",
        )
        .await
        .unwrap();
    setup
        .execute("CREATE TYPE shop.mood AS ENUM ('happy', 'sad')")
        .await
        .unwrap();
    setup
        .execute("CREATE TABLE shop.records (id integer PRIMARY KEY)")
        .await
        .unwrap();
    setup.execute("INSERT INTO shop.records VALUES (1)").await.unwrap();
    setup
        .execute(
            "CREATE FUNCTION shadow.array_step(text, anynonarray) RETURNS text \
             LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'shadow aggregate invoked'; END $$",
        )
        .await
        .unwrap();
    setup
        .execute(
            "CREATE AGGREGATE shadow.array_agg(anynonarray) \
             (SFUNC = shadow.array_step, STYPE = text)",
        )
        .await
        .unwrap();
    setup
        .execute("ALTER ROLE postgres SET search_path TO shadow, pg_catalog")
        .await
        .unwrap();
    drop(setup);

    let connection = PgDriver.connect(options).await.unwrap();
    let path_before = search_path(connection.as_ref()).await;

    let type_result = connection.query("SELECT 'happy'::shop.mood").await.unwrap();

    assert_eq!(type_result.rows[0][0], bookie_core::Value::Text("happy".into()));
    let column_result = connection.query("SELECT id FROM shop.records").await.unwrap();
    assert_eq!(column_result.rows[0][0], bookie_core::Value::Int(1));
    assert_eq!(search_path(connection.as_ref()).await, path_before);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn sqlx_column_resolution_ignores_a_shadowed_array_agg() {
    let container = PostgresImage::default().with_tag("16-alpine").start().await.unwrap();
    let options = options(
        &container.get_host().await.unwrap().to_string(),
        container.get_host_port_ipv4(5432).await.unwrap(),
        "postgres",
    );
    let setup = PgDriver.connect(options.clone()).await.unwrap();
    setup.execute("CREATE SCHEMA shadow").await.unwrap();
    setup.execute("CREATE SCHEMA shop").await.unwrap();
    setup
        .execute("CREATE TABLE shop.records (id integer PRIMARY KEY)")
        .await
        .unwrap();
    setup.execute("INSERT INTO shop.records VALUES (1)").await.unwrap();
    setup
        .execute(
            "CREATE FUNCTION shadow.array_step(text, anynonarray) RETURNS text \
             LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'shadow aggregate invoked'; END $$",
        )
        .await
        .unwrap();
    setup
        .execute("CREATE AGGREGATE shadow.array_agg(anynonarray) (SFUNC = shadow.array_step, STYPE = text)")
        .await
        .unwrap();
    setup
        .execute("ALTER ROLE postgres SET search_path TO shadow, pg_catalog")
        .await
        .unwrap();
    drop(setup);

    let connection = PgDriver.connect(options).await.unwrap();
    let path_before = search_path(connection.as_ref()).await;
    let result = connection.query("SELECT id FROM shop.records").await.unwrap();

    assert_eq!(result.rows[0][0], bookie_core::Value::Int(1));
    assert_eq!(search_path(connection.as_ref()).await, path_before);
}
