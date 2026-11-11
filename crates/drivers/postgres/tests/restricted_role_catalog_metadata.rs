#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{CatalogObjectKind, ConnectOptions, DatabaseDriver};
use drivers_postgres::PgDriver;

fn options(user: &str, password: &str) -> ConnectOptions {
    let port = std::env::var("VISIBILITY_PG_PORT").expect("VISIBILITY_PG_PORT");
    ConnectOptions {
        host: std::env::var("VISIBILITY_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: port.parse().expect("VISIBILITY_PG_PORT is a port"),
        database: std::env::var("VISIBILITY_PG_DB").unwrap_or_else(|_| "postgres".into()),
        username: user.into(),
        password: secrecy::SecretString::new(password.to_string().into()),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    }
}

const SEED: &[&str] = &[
    "DROP SCHEMA IF EXISTS bookie_meta_visible CASCADE",
    "DROP SCHEMA IF EXISTS bookie_meta_hidden CASCADE",
    "DROP SCHEMA IF EXISTS \"Bookie Meta Quoted\" CASCADE",
    "DROP ROLE IF EXISTS bookie_meta_restricted",
    "CREATE ROLE bookie_meta_restricted LOGIN PASSWORD 'restricted'",
    "CREATE SCHEMA bookie_meta_visible",
    "CREATE SCHEMA bookie_meta_hidden",
    "CREATE SCHEMA \"Bookie Meta Quoted\"",
    "CREATE TABLE bookie_meta_visible.shown (id int PRIMARY KEY, note text)",
    "CREATE TABLE bookie_meta_visible.denied (id int, secret text)",
    "CREATE TABLE bookie_meta_visible.column_only (id int, secret text)",
    "CREATE TABLE bookie_meta_hidden.secret (id int PRIMARY KEY, hidden_column text)",
    "CREATE TABLE bookie_meta_visible.child (ref int REFERENCES bookie_meta_hidden.secret (id))",
    "CREATE TABLE \"Bookie Meta Quoted\".\"Mixed \"\"Case\"\"\" (\"Id\" int, \"Other Col\" text)",
    "CREATE EXTENSION IF NOT EXISTS citext WITH SCHEMA bookie_meta_hidden",
    "GRANT USAGE ON SCHEMA bookie_meta_visible, \"Bookie Meta Quoted\" TO bookie_meta_restricted",
    "GRANT SELECT ON bookie_meta_visible.shown TO bookie_meta_restricted",
    "GRANT SELECT (id) ON bookie_meta_visible.column_only TO bookie_meta_restricted",
    "GRANT SELECT ON bookie_meta_visible.child TO bookie_meta_restricted",
    "GRANT SELECT ON \"Bookie Meta Quoted\".\"Mixed \"\"Case\"\"\" TO bookie_meta_restricted",
];

const CLEANUP: &[&str] = &[
    "DROP EXTENSION IF EXISTS citext CASCADE",
    "DROP SCHEMA IF EXISTS bookie_meta_visible CASCADE",
    "DROP SCHEMA IF EXISTS bookie_meta_hidden CASCADE",
    "DROP SCHEMA IF EXISTS \"Bookie Meta Quoted\" CASCADE",
    "DROP ROLE IF EXISTS bookie_meta_restricted",
];

async fn column_names(driver: &dyn bookie_core::Connection, schema: &str, table: &str) -> Vec<String> {
    match driver.fetch_columns(Some(schema), table).await {
        Ok(columns) => columns.into_iter().map(|column| column.name).collect(),
        Err(error) => vec![format!("ERR {error}")],
    }
}

async fn index_names(driver: &dyn bookie_core::Connection, schema: &str, table: &str) -> Vec<String> {
    driver
        .fetch_indexes(Some(schema), table)
        .await
        .expect("indexes")
        .into_iter()
        .map(|index| index.name)
        .collect()
}

#[tokio::test]
#[ignore = "requires a PostgreSQL superuser; see restricted_role_catalog.rs"]
async fn a_restricted_role_reads_no_metadata_of_objects_it_cannot_reach() {
    let admin_user = std::env::var("VISIBILITY_PG_ADMIN").expect("VISIBILITY_PG_ADMIN");
    let admin_pass = std::env::var("VISIBILITY_PG_ADMIN_PASS").expect("VISIBILITY_PG_ADMIN_PASS");
    let admin = PgDriver
        .connect(options(&admin_user, &admin_pass))
        .await
        .expect("admin");
    for statement in CLEANUP.iter().chain(SEED) {
        admin.execute(statement).await.expect(statement);
    }
    let restricted = PgDriver
        .connect(options("bookie_meta_restricted", "restricted"))
        .await
        .expect("restricted");

    let outcome = async {
        let extensions = restricted
            .list_objects(CatalogObjectKind::Extension, None)
            .await
            .expect("extensions");
        let extension_names: Vec<String> = extensions.iter().map(|object| object.name.clone()).collect();
        let mut columns = Vec::new();
        for (schema, table) in [
            ("bookie_meta_visible", "shown"),
            ("bookie_meta_visible", "denied"),
            ("bookie_meta_visible", "column_only"),
            ("bookie_meta_hidden", "secret"),
            ("Bookie Meta Quoted", "Mixed \"Case\""),
        ] {
            columns.push(column_names(restricted.as_ref(), schema, table).await);
        }
        let indexes = (
            index_names(restricted.as_ref(), "bookie_meta_visible", "shown").await,
            index_names(restricted.as_ref(), "bookie_meta_hidden", "secret").await,
        );
        let restricted_foreign_keys = restricted
            .fetch_foreign_keys(Some("bookie_meta_visible"), "child")
            .await
            .expect("restricted foreign keys");
        let admin_foreign_keys = admin
            .fetch_foreign_keys(Some("bookie_meta_visible"), "child")
            .await
            .expect("admin foreign keys");
        let admin_columns = column_names(admin.as_ref(), "bookie_meta_hidden", "secret").await;
        (
            extension_names,
            columns,
            indexes,
            restricted_foreign_keys.len(),
            admin_foreign_keys.len(),
            admin_columns,
        )
    }
    .await;
    for statement in CLEANUP {
        admin.execute(statement).await.expect(statement);
    }
    let (extension_names, columns, indexes, restricted_foreign_keys, admin_foreign_keys, admin_columns) = outcome;

    assert!(
        extension_names.iter().any(|name| name == "plpgsql"),
        "{extension_names:?}"
    );
    assert!(
        !extension_names.iter().any(|name| name == "citext"),
        "an extension in a schema the role cannot use must not be listed: {extension_names:?}"
    );
    assert_eq!(columns[0], ["id", "note"], "a reachable table keeps its columns");
    assert!(
        columns[1].is_empty(),
        "a table with no privilege has no readable columns: {:?}",
        columns[1]
    );
    assert_eq!(
        columns[2],
        ["id"],
        "only the column the role may read: {:?}",
        columns[2]
    );
    assert!(
        columns[3].is_empty(),
        "a table in an unusable schema has no readable columns: {:?}",
        columns[3]
    );
    assert_eq!(columns[4], ["Id", "Other Col"], "quoted identifiers stay exact");
    assert_eq!(indexes.0, ["shown_pkey"]);
    assert!(
        indexes.1.is_empty(),
        "no index facts for an unreachable table: {:?}",
        indexes.1
    );
    assert_eq!(
        restricted_foreign_keys, 0,
        "a foreign key to an unreachable table is not reported"
    );
    assert_eq!(admin_foreign_keys, 1, "the owner still sees it");
    assert_eq!(
        admin_columns,
        ["id", "hidden_column"],
        "an administrator still reads everything"
    );
}
