#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{CommitResolution, CommitToken, ConnectOptions, DatabaseDriver, TlsConfig};
use drivers_postgres::PgDriver;
use sqlx::Connection as _;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres as PostgresImage;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

async fn xid_of(raw: &mut sqlx::PgConnection) -> u64 {
    let text: String = sqlx::query_scalar("SELECT pg_catalog.pg_current_xact_id()::text")
        .fetch_one(&mut *raw)
        .await
        .unwrap();
    text.parse().unwrap()
}

fn token(xid: u64) -> CommitToken {
    CommitToken::PostgresXid {
        xid,
        legacy_txid: false,
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn the_server_says_what_became_of_a_transaction_by_its_id() {
    let container = PostgresImage::default().with_tag("16-alpine").start().await.unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres?sslmode=disable");
    let connection = PgDriver
        .connect(ConnectOptions {
            host,
            port,
            database: "postgres".into(),
            username: "postgres".into(),
            password: secrecy::SecretString::new("postgres".to_string().into()),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .unwrap();

    let mut raw = sqlx::PgConnection::connect(&url).await.unwrap();
    let mut committed = raw.begin().await.unwrap();
    let committed_xid = xid_of(&mut committed).await;
    let mut other = sqlx::PgConnection::connect(&url).await.unwrap();
    let mut running = other.begin().await.unwrap();
    let running_xid = xid_of(&mut running).await;
    assert_eq!(
        connection.resolve_commit(&token(running_xid)).await.unwrap(),
        CommitResolution::StillRunning
    );
    committed.commit().await.unwrap();
    assert_eq!(
        connection.resolve_commit(&token(committed_xid)).await.unwrap(),
        CommitResolution::Applied
    );
    running.rollback().await.unwrap();
    assert_eq!(
        connection.resolve_commit(&token(running_xid)).await.unwrap(),
        CommitResolution::NotApplied
    );
    assert_eq!(
        connection.resolve_commit(&token(3)).await.unwrap(),
        CommitResolution::Unresolvable,
        "an id from before the commit log's horizon has no record"
    );
}
