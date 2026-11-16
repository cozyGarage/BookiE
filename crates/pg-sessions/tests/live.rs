#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::time::Duration;

use bookie_core::{ConnectOptions, DatabaseDriver, TlsConfig};
use bookie_pg_sessions::{blockers, list_sessions};
use drivers_postgres::PgDriver;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres as PostgresImage;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

fn options(host: String, port: u16) -> ConnectOptions {
    ConnectOptions {
        host,
        port,
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_session_waiting_on_a_row_lock_names_its_blocker() {
    let container = PostgresImage::default().with_tag("16-alpine").start().await.unwrap();
    let options = ConnectOptions {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(5432).await.unwrap(),
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let setup = PgDriver.connect(options.clone()).await.unwrap();
    setup.execute("CREATE TABLE held (id int PRIMARY KEY)").await.unwrap();
    setup.execute("INSERT INTO held VALUES (1)").await.unwrap();

    let holder_connection = PgDriver.connect(options.clone()).await.unwrap();
    let mut holder = holder_connection.begin().await.unwrap();
    holder.execute("UPDATE held SET id = 1 WHERE id = 1").await.unwrap();
    let waiter = PgDriver.connect(options.clone()).await.unwrap();
    let waiting = tokio::spawn(async move { waiter.execute("UPDATE held SET id = 1 WHERE id = 1").await });
    tokio::time::sleep(Duration::from_secs(2)).await;

    let sessions = list_sessions(&*setup).await.unwrap();
    let ranked = blockers(&sessions);
    assert_eq!(ranked.len(), 1, "{sessions:#?}");
    let blocker = sessions
        .iter()
        .find(|s| s.pid == ranked[0].0)
        .expect("the blocker is listed");
    assert!(blocker.query.contains("UPDATE held"), "{blocker:?}");
    assert!(
        sessions
            .iter()
            .any(|s| s.blocked_by == [blocker.pid] && s.wait.as_deref().is_some_and(|w| w.starts_with("Lock")))
    );

    holder.commit().await.unwrap();
    waiting.await.unwrap().unwrap();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn session_metadata_ignores_objects_shadowing_pg_catalog_on_search_path() {
    let container = PostgresImage::default().with_tag("16-alpine").start().await.unwrap();
    let options = options(
        container.get_host().await.unwrap().to_string(),
        container.get_host_port_ipv4(5432).await.unwrap(),
    );
    let setup = PgDriver.connect(options.clone()).await.unwrap();
    for statement in [
        "CREATE SCHEMA shadow",
        "CREATE TYPE shadow.text AS ENUM ('shadow')",
        "CREATE TYPE shadow.bigint AS ENUM ('shadow')",
        "CREATE TABLE shadow.pg_stat_activity (pid int, usename text, datname text, state text, \
         wait_event_type text, wait_event text, query_start timestamptz, query text, backend_type text)",
        "INSERT INTO shadow.pg_stat_activity VALUES (1, 'shadow', 'postgres', 'active', NULL, NULL, \
         '2000-01-01 00:00:00+00', 'shadow fake query', 'client backend')",
        "CREATE FUNCTION shadow.now() RETURNS timestamptz LANGUAGE sql IMMUTABLE AS \
         $$ SELECT '2000-01-01 00:00:00+00'::timestamptz $$",
        "CREATE FUNCTION shadow.pg_blocking_pids(integer) RETURNS integer[] LANGUAGE sql IMMUTABLE AS \
         $$ SELECT ARRAY[]::integer[] $$",
        "CREATE FUNCTION shadow.array_to_string(integer[], text) RETURNS text LANGUAGE sql IMMUTABLE AS \
         $$ SELECT 'shadow blocker'::text $$",
        "CREATE FUNCTION shadow.left(text, integer) RETURNS text LANGUAGE sql IMMUTABLE AS \
         $$ SELECT 'shadow query'::text $$",
        "ALTER ROLE postgres SET search_path TO shadow, pg_catalog",
    ] {
        setup.execute(statement).await.expect(statement);
    }
    drop(setup);

    let holder = PgDriver.connect(options.clone()).await.unwrap();
    let holder_query = tokio::spawn(async move {
        holder
            .query("SELECT pg_catalog.pg_sleep(3) /* bookie_session_shadow_marker */")
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let monitor = PgDriver.connect(options).await.unwrap();
    let path_before = monitor.query("SHOW search_path").await.unwrap().rows;

    let sessions = list_sessions(monitor.as_ref()).await.unwrap();
    holder_query.await.unwrap().unwrap();

    assert!(
        sessions
            .iter()
            .any(|session| session.query.contains("bookie_session_shadow_marker")),
        "{sessions:#?}"
    );
    let marker = sessions
        .iter()
        .find(|session| session.query.contains("bookie_session_shadow_marker"))
        .unwrap();
    assert!((0..=60).contains(&marker.running_secs), "{marker:?}");
    assert!(!sessions.iter().any(|session| session.query == "shadow fake query"));
    assert_eq!(monitor.query("SHOW search_path").await.unwrap().rows, path_before);
}
