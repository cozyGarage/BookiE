#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Session temporary tables against an already-running MySQL or MariaDB (no Docker required).
//!
//! Ignored by default. Env: SMOKE_MYSQL_HOST, SMOKE_MYSQL_PORT, SMOKE_MYSQL_USER, SMOKE_MYSQL_PASS,
//! SMOKE_MYSQL_DB, and SMOKE_MYSQL_LISTS_TEMP=1 for MariaDB (lists them) or 0 for MySQL (cannot).

use bookie_core::{ConnectOptions, DatabaseDriver, OperationControl};
use drivers_mysql::MysqlDriver;

fn opts_from_env() -> ConnectOptions {
    ConnectOptions {
        host: std::env::var("SMOKE_MYSQL_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: std::env::var("SMOKE_MYSQL_PORT")
            .unwrap_or_else(|_| "3306".into())
            .parse()
            .unwrap(),
        database: std::env::var("SMOKE_MYSQL_DB").unwrap_or_else(|_| "mysql".into()),
        username: std::env::var("SMOKE_MYSQL_USER").unwrap_or_else(|_| "root".into()),
        password: secrecy::SecretString::new(std::env::var("SMOKE_MYSQL_PASS").unwrap_or_default().into()),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    }
}

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(10))
}

#[tokio::test]
#[ignore = "requires a running MySQL or MariaDB; set SMOKE_MYSQL_*"]
async fn a_session_lists_its_own_temporary_tables_where_the_server_exposes_them() {
    let lists = std::env::var("SMOKE_MYSQL_LISTS_TEMP").is_ok_and(|value| value == "1");
    let connection = MysqlDriver.connect(opts_from_env()).await.unwrap();
    connection
        .execute("CREATE DATABASE IF NOT EXISTS claude_temp_probe")
        .await
        .unwrap();
    let mut first = connection.open_session().await.unwrap();
    first
        .query_params_controlled(
            "CREATE TEMPORARY TABLE claude_temp_probe.claude_tmp_a (a INT)",
            &[],
            &control(),
        )
        .await
        .unwrap();
    let mut second = connection.open_session().await.unwrap();

    let seen_by_owner = first.list_temporary_tables_controlled(&control()).await.unwrap();
    let seen_by_other = second.list_temporary_tables_controlled(&control()).await.unwrap();

    let names: Vec<(Option<&str>, &str)> = seen_by_owner
        .iter()
        .map(|table| (table.schema.as_deref(), table.name.as_str()))
        .collect();
    if lists {
        assert_eq!(names, [(Some("claude_temp_probe"), "claude_tmp_a")]);
    } else {
        assert!(
            names.is_empty(),
            "this server does not expose temporary tables: {names:?}"
        );
    }
    assert!(
        seen_by_other.is_empty(),
        "another session must not see the temporary table"
    );

    first.close().await.unwrap();
    second.close().await.unwrap();
    connection.execute("DROP DATABASE claude_temp_probe").await.unwrap();
}
