#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A script run in a chosen schema, against already-running servers (no Docker required).
//!
//! Ignored by default. PostgreSQL: SMOKE_PG_HOST, SMOKE_PG_PORT, SMOKE_PG_USER, SMOKE_PG_PASS, SMOKE_PG_DB.
//! MySQL or MariaDB: SMOKE_MYSQL_HOST, SMOKE_MYSQL_PORT, SMOKE_MYSQL_USER, SMOKE_MYSQL_PASS, SMOKE_MYSQL_DB, and
//! SMOKE_MYSQL_TLS=require for MySQL 8, whose default password exchange the driver refuses to send without TLS.
//! Each test creates and drops its own scratch schema or database.

use bookie_app_services::schema_context::schema_context_statement;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, OperationControl, Session, Value};

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(10))
}

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.into())
}

fn postgres_options() -> ConnectOptions {
    ConnectOptions {
        host: env("SMOKE_PG_HOST", "127.0.0.1"),
        port: env("SMOKE_PG_PORT", "54329").parse().unwrap(),
        database: env("SMOKE_PG_DB", "bookie"),
        username: env("SMOKE_PG_USER", "bookie"),
        password: secrecy::SecretString::new(env("SMOKE_PG_PASS", "bookie").into()),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    }
}

fn mysql_options() -> ConnectOptions {
    ConnectOptions {
        host: env("SMOKE_MYSQL_HOST", "127.0.0.1"),
        port: env("SMOKE_MYSQL_PORT", "3306").parse().unwrap(),
        database: env("SMOKE_MYSQL_DB", "mysql"),
        username: env("SMOKE_MYSQL_USER", "root"),
        password: secrecy::SecretString::new(env("SMOKE_MYSQL_PASS", "").into()),
        tls: if env("SMOKE_MYSQL_TLS", "") == "require" {
            bookie_core::TlsConfig {
                mode: bookie_core::TlsMode::Require,
                ..Default::default()
            }
        } else {
            bookie_core::TlsConfig::disabled()
        },
        ..Default::default()
    }
}

async fn text(session: &mut Box<dyn Session>, sql: &str) -> String {
    let result = session.query_params_controlled(sql, &[], &control()).await.unwrap();
    match result.rows.first().and_then(|row| row.first()) {
        Some(Value::Text(value)) => value.clone(),
        other => panic!("{sql}: expected one text cell, got {other:?}"),
    }
}

async fn run(session: &mut Box<dyn Session>, sql: &str) {
    session.query_params_controlled(sql, &[], &control()).await.unwrap();
}

async fn scripts_land_in_the_chosen_place(
    connection: Box<dyn Connection>,
    driver_id: &str,
    scratch: &str,
    create: &str,
    drop: &str,
    where_table_lives: &str,
    current_default: &str,
) {
    connection.execute(drop).await.ok();
    connection.execute(create).await.unwrap();
    let mut chosen = connection.open_session().await.unwrap();
    let default_before = text(&mut chosen, current_default).await;
    run(&mut chosen, &schema_context_statement(driver_id, scratch).unwrap()).await;
    run(&mut chosen, "CREATE TABLE claude_ctx_probe (a INT)").await;
    assert_eq!(
        text(&mut chosen, where_table_lives).await,
        scratch,
        "the table landed elsewhere"
    );
    assert_eq!(
        text(&mut chosen, current_default).await,
        scratch,
        "the session did not switch"
    );

    let mut untouched = connection.open_session().await.unwrap();
    assert_eq!(
        text(&mut untouched, current_default).await,
        default_before,
        "another session must keep the server default"
    );
    chosen.close().await.unwrap();
    untouched.close().await.unwrap();
    connection.execute(drop).await.unwrap();
}

#[tokio::test]
#[ignore = "requires a running PostgreSQL; set SMOKE_PG_*"]
async fn postgres_creates_objects_in_the_chosen_schema_including_an_awkward_name() {
    for scratch in ["claude_ctx_plain", "Claude\"Odd Name"] {
        let connection = PgDriver.connect(postgres_options()).await.unwrap();
        let quoted = bookie_core::sql_dialect::quote_ident("postgres", scratch);
        scripts_land_in_the_chosen_place(
            connection,
            "postgres",
            scratch,
            &format!("CREATE SCHEMA {quoted}"),
            &format!("DROP SCHEMA IF EXISTS {quoted} CASCADE"),
            "SELECT table_schema::text FROM information_schema.tables WHERE table_name = 'claude_ctx_probe'",
            "SELECT current_schema()::text",
        )
        .await;
    }
}

async fn mysql_family(driver_options: ConnectOptions) {
    let connection = MysqlDriver.connect(driver_options).await.unwrap();
    scripts_land_in_the_chosen_place(
        connection,
        "mysql",
        "claude_ctx_db",
        "CREATE DATABASE claude_ctx_db",
        "DROP DATABASE IF EXISTS claude_ctx_db",
        "SELECT CAST(table_schema AS CHAR) FROM information_schema.tables WHERE table_name = 'claude_ctx_probe'",
        "SELECT COALESCE(DATABASE(), '')",
    )
    .await;
}

#[tokio::test]
#[ignore = "requires a running MySQL or MariaDB; set SMOKE_MYSQL_*"]
async fn mysql_and_mariadb_create_objects_in_the_chosen_database() {
    mysql_family(mysql_options()).await;
}

use drivers_mysql::MysqlDriver;
use drivers_postgres::PgDriver;
