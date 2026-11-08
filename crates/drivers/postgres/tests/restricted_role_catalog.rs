#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A restricted role lists only the schemas and relations it can reach (ADR 0034).
//!
//! Ignored by default: it needs a PostgreSQL superuser and creates then drops the schemas `bookie_vis_*` and
//! the role `bookie_vis_restricted`. Point it at a scratch server:
//!
//! ```text
//! VISIBILITY_PG_HOST=127.0.0.1 VISIBILITY_PG_PORT=54339 VISIBILITY_PG_ADMIN=postgres VISIBILITY_PG_ADMIN_PASS=admin \
//!   cargo test -p bookie-driver-postgres --test restricted_role_catalog -- --include-ignored
//! ```

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
    "DROP SCHEMA IF EXISTS bookie_vis_visible CASCADE",
    "DROP SCHEMA IF EXISTS bookie_vis_hidden CASCADE",
    "DROP SCHEMA IF EXISTS bookie_vis_usage CASCADE",
    "DROP ROLE IF EXISTS bookie_vis_restricted",
    "CREATE ROLE bookie_vis_restricted LOGIN PASSWORD 'restricted'",
    "CREATE SCHEMA bookie_vis_visible",
    "CREATE SCHEMA bookie_vis_hidden",
    "CREATE SCHEMA bookie_vis_usage",
    "CREATE TABLE bookie_vis_visible.shown (id int PRIMARY KEY)",
    "CREATE TABLE bookie_vis_visible.denied (id int)",
    "CREATE VIEW bookie_vis_visible.shown_view AS SELECT id FROM bookie_vis_visible.shown",
    "CREATE MATERIALIZED VIEW bookie_vis_visible.shown_mat AS SELECT id FROM bookie_vis_visible.shown",
    "CREATE VIEW bookie_vis_visible.denied_view AS SELECT id FROM bookie_vis_visible.denied",
    "CREATE TABLE bookie_vis_hidden.secret (id int)",
    "CREATE VIEW bookie_vis_hidden.secret_view AS SELECT id FROM bookie_vis_hidden.secret",
    "CREATE FUNCTION bookie_vis_hidden.secret_fn() RETURNS int LANGUAGE sql AS 'SELECT 1'",
    "CREATE FUNCTION bookie_vis_hidden.secret_trg() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'",
    "CREATE TRIGGER secret_trg BEFORE INSERT ON bookie_vis_hidden.secret FOR EACH ROW EXECUTE FUNCTION bookie_vis_hidden.secret_trg()",
    "CREATE SEQUENCE bookie_vis_hidden.secret_seq",
    "CREATE TYPE bookie_vis_hidden.secret_type AS ENUM ('x')",
    "CREATE TABLE bookie_vis_usage.t (id int)",
    "GRANT USAGE ON SCHEMA bookie_vis_visible TO bookie_vis_restricted",
    "GRANT SELECT ON bookie_vis_visible.shown, bookie_vis_visible.shown_view, bookie_vis_visible.shown_mat TO bookie_vis_restricted",
    "GRANT USAGE ON SCHEMA bookie_vis_usage TO bookie_vis_restricted",
    "CREATE FUNCTION bookie_vis_visible.open_fn() RETURNS int LANGUAGE sql AS 'SELECT 1'",
    "CREATE FUNCTION bookie_vis_visible.closed_fn() RETURNS int LANGUAGE sql AS 'SELECT 1'",
    "REVOKE EXECUTE ON FUNCTION bookie_vis_visible.closed_fn() FROM PUBLIC",
    "CREATE FUNCTION bookie_vis_visible.trg() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'",
    "CREATE TRIGGER open_trg BEFORE INSERT ON bookie_vis_visible.shown FOR EACH ROW EXECUTE FUNCTION bookie_vis_visible.trg()",
    "CREATE TRIGGER closed_trg BEFORE INSERT ON bookie_vis_visible.denied FOR EACH ROW EXECUTE FUNCTION bookie_vis_visible.trg()",
    "CREATE SEQUENCE bookie_vis_visible.open_seq",
    "CREATE SEQUENCE bookie_vis_visible.closed_seq",
    "GRANT USAGE ON SEQUENCE bookie_vis_visible.open_seq TO bookie_vis_restricted",
    "CREATE TYPE bookie_vis_visible.open_type AS ENUM ('a')",
    "CREATE TYPE bookie_vis_visible.closed_type AS ENUM ('a')",
    "REVOKE USAGE ON TYPE bookie_vis_visible.closed_type FROM PUBLIC",
];

const CLEANUP: &[&str] = &[
    "DROP SCHEMA IF EXISTS bookie_vis_visible CASCADE",
    "DROP SCHEMA IF EXISTS bookie_vis_hidden CASCADE",
    "DROP SCHEMA IF EXISTS bookie_vis_usage CASCADE",
    "DROP ROLE IF EXISTS bookie_vis_restricted",
];

fn object_names(objects: &[bookie_core::CatalogObject]) -> Vec<String> {
    objects
        .iter()
        .filter(|object| {
            object
                .schema
                .as_deref()
                .is_some_and(|schema| schema.starts_with("bookie_vis_"))
        })
        .map(|object| format!("{}.{}", object.schema.as_deref().unwrap_or_default(), object.name))
        .collect()
}

fn names(objects: &[bookie_core::TableInfo]) -> Vec<String> {
    objects
        .iter()
        .filter(|object| {
            object
                .schema
                .as_deref()
                .is_some_and(|schema| schema.starts_with("bookie_vis_"))
        })
        .map(|object| format!("{}.{}", object.schema.as_deref().unwrap_or_default(), object.name))
        .collect()
}

async fn excluded_names(
    connection: &dyn bookie_core::Connection,
    hidden_schemas: &[String],
) -> Result<(Vec<String>, Vec<String>, Vec<Vec<String>>), bookie_core::DriverError> {
    let tables = names(&connection.list_tables_excluding(hidden_schemas).await?);
    let views = names(&connection.list_views_excluding(hidden_schemas).await?);
    let mut objects = Vec::new();
    for kind in CatalogObjectKind::ALL {
        let listed = connection.list_objects_excluding(kind, None, hidden_schemas).await?;
        objects.push(object_names(&listed));
    }
    Ok((tables, views, objects))
}

#[tokio::test]
#[ignore = "requires a PostgreSQL superuser; see the module comment"]
async fn a_restricted_role_lists_only_what_it_can_reach_and_an_admin_lists_everything() {
    let admin_user = std::env::var("VISIBILITY_PG_ADMIN").expect("VISIBILITY_PG_ADMIN");
    let admin_pass = std::env::var("VISIBILITY_PG_ADMIN_PASS").expect("VISIBILITY_PG_ADMIN_PASS");
    let admin = PgDriver
        .connect(options(&admin_user, &admin_pass))
        .await
        .expect("admin connects");
    for statement in SEED {
        admin.execute(statement).await.expect(statement);
    }
    let restricted = PgDriver
        .connect(options("bookie_vis_restricted", "restricted"))
        .await
        .expect("restricted connects");

    let outcome = async {
        let tables = names(&restricted.list_tables().await.expect("restricted tables"));
        let views = names(&restricted.list_views().await.expect("restricted views"));
        let admin_tables = names(&admin.list_tables().await.expect("admin tables"));
        let hidden_schemas = ["bookie_vis_hidden".to_string()];
        let (excluded_tables, excluded_views, excluded_objects) = excluded_names(admin.as_ref(), &hidden_schemas)
            .await
            .expect("catalog listings excluding hidden schemas");
        let scoped = names(
            &restricted
                .list_tables_scoped(&["bookie_vis_visible".to_string(), "bookie_vis_hidden".to_string()])
                .await
                .expect("scoped tables"),
        );
        let admin_scoped = names(
            &admin
                .list_tables_scoped(&["bookie_vis_usage".to_string()])
                .await
                .expect("admin scoped tables"),
        );
        let admin_views = names(&admin.list_views().await.expect("admin views"));
        let mut objects = Vec::new();
        for kind in [
            CatalogObjectKind::Routine,
            CatalogObjectKind::Trigger,
            CatalogObjectKind::Sequence,
            CatalogObjectKind::Type,
            CatalogObjectKind::Grant,
        ] {
            let listed = restricted.list_objects(kind, None).await.expect("restricted objects");
            objects.push(object_names(&listed));
        }
        (
            tables,
            views,
            admin_tables,
            admin_views,
            objects,
            scoped,
            admin_scoped,
            excluded_tables,
            excluded_views,
            excluded_objects,
        )
    }
    .await;
    for statement in CLEANUP {
        admin.execute(statement).await.expect(statement);
    }
    let (
        tables,
        views,
        admin_tables,
        admin_views,
        objects,
        scoped,
        admin_scoped,
        excluded_tables,
        excluded_views,
        excluded_objects,
    ) = outcome;

    assert_eq!(
        tables,
        ["bookie_vis_visible.shown"],
        "no hidden, denied or usage-only table"
    );
    assert_eq!(
        views,
        ["bookie_vis_visible.shown_mat", "bookie_vis_visible.shown_view"],
        "a view or materialized view the role cannot read is not listed"
    );
    assert_eq!(
        admin_tables,
        [
            "bookie_vis_hidden.secret",
            "bookie_vis_usage.t",
            "bookie_vis_visible.denied",
            "bookie_vis_visible.shown",
        ]
    );
    assert_eq!(admin_views.len(), 4, "{admin_views:?}");
    assert_eq!(excluded_tables.len(), 3, "{excluded_tables:?}");
    assert_eq!(excluded_views.len(), 3, "{excluded_views:?}");
    for objects in excluded_objects {
        assert!(
            objects.iter().all(|name| !name.starts_with("bookie_vis_hidden.")),
            "hidden-schema objects remain visible: {objects:?}"
        );
    }
    assert_eq!(
        objects[0],
        ["bookie_vis_visible.open_fn", "bookie_vis_visible.trg"],
        "a routine the role cannot execute is not listed"
    );
    assert_eq!(
        objects[1],
        ["bookie_vis_visible.open_trg"],
        "a trigger on a table the role cannot reach is not listed"
    );
    assert_eq!(
        objects[2],
        ["bookie_vis_visible.open_seq"],
        "a sequence the role cannot use is not listed"
    );
    assert_eq!(
        objects[3],
        ["bookie_vis_visible.open_type"],
        "a type the role cannot use is not listed"
    );
    assert!(
        objects[4]
            .iter()
            .all(|name| !name.contains("denied") && !name.contains("secret") && !name.ends_with(".t")),
        "grants of unreachable relations are not listed: {:?}",
        objects[4]
    );
    assert!(!objects[4].is_empty(), "grants of reachable relations still are");
    assert_eq!(
        scoped,
        ["bookie_vis_visible.shown"],
        "scoping to two schemas keeps the visibility rule: the hidden schema and denied table stay out"
    );
    assert_eq!(
        admin_scoped,
        ["bookie_vis_usage.t"],
        "scoping returns only the asked schema"
    );
}
