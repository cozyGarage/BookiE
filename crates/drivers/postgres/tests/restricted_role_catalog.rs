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

use bookie_core::{ConnectOptions, DatabaseDriver};
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
    "CREATE TABLE bookie_vis_usage.t (id int)",
    "GRANT USAGE ON SCHEMA bookie_vis_visible TO bookie_vis_restricted",
    "GRANT SELECT ON bookie_vis_visible.shown, bookie_vis_visible.shown_view, bookie_vis_visible.shown_mat TO bookie_vis_restricted",
    "GRANT USAGE ON SCHEMA bookie_vis_usage TO bookie_vis_restricted",
];

const CLEANUP: &[&str] = &[
    "DROP SCHEMA IF EXISTS bookie_vis_visible CASCADE",
    "DROP SCHEMA IF EXISTS bookie_vis_hidden CASCADE",
    "DROP SCHEMA IF EXISTS bookie_vis_usage CASCADE",
    "DROP ROLE IF EXISTS bookie_vis_restricted",
];

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
        let admin_views = names(&admin.list_views().await.expect("admin views"));
        (tables, views, admin_tables, admin_views)
    }
    .await;
    for statement in CLEANUP {
        admin.execute(statement).await.expect(statement);
    }
    let (tables, views, admin_tables, admin_views) = outcome;

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
    assert_eq!(admin_views.len(), 3, "{admin_views:?}");
}
