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

const COMMIT_MESSAGE: &[u8] = b"Q\0\0\0\x0bCOMMIT\0";

fn contains_commit(chunk: &[u8]) -> bool {
    chunk
        .windows(COMMIT_MESSAGE.len())
        .any(|window| window == COMMIT_MESSAGE)
}

/// Relays one connection, then cuts it when the client sends COMMIT: the
/// server gets the COMMIT only when `deliver` is true, and the client never
/// sees a reply either way.
async fn cut_at_commit(listener: tokio::net::TcpListener, upstream: std::net::SocketAddr, deliver: bool) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (client, _) = listener.accept().await.unwrap();
    let server = tokio::net::TcpStream::connect(upstream).await.unwrap();
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();
    let cut = Arc::new(AtomicBool::new(false));
    let relay_cut = cut.clone();
    let back = tokio::spawn(async move {
        let mut buffer = [0u8; 8192];
        while let Ok(count) = server_read.read(&mut buffer).await {
            if count == 0 || relay_cut.load(Ordering::SeqCst) {
                break;
            }
            if client_write.write_all(&buffer[..count]).await.is_err() {
                break;
            }
        }
    });
    let mut buffer = [0u8; 8192];
    while let Ok(count) = client_read.read(&mut buffer).await {
        if count == 0 {
            break;
        }
        if contains_commit(&buffer[..count]) {
            cut.store(true, Ordering::SeqCst);
            if deliver {
                server_write.write_all(&buffer[..count]).await.unwrap();
                server_write.flush().await.unwrap();
            }
            break;
        }
        if server_write.write_all(&buffer[..count]).await.is_err() {
            break;
        }
    }
    back.abort();
}

async fn settle(connection: &dyn bookie_core::Connection, token: &CommitToken) -> CommitResolution {
    for _ in 0..50 {
        let answer = connection.resolve_commit(token).await.unwrap();
        if answer != CommitResolution::StillRunning {
            return answer;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    CommitResolution::StillRunning
}

async fn lose_the_commit(deliver: bool) -> (CommitResolution, i64) {
    let container = PostgresImage::default().with_tag("16-alpine").start().await.unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let options = |port: u16| ConnectOptions {
        host: host.clone(),
        port,
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let direct = PgDriver.connect(options(port)).await.unwrap();
    direct.execute("CREATE TABLE ledger (id int)").await.unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_port = listener.local_addr().unwrap().port();
    let upstream = tokio::net::lookup_host((host.as_str(), port))
        .await
        .unwrap()
        .next()
        .unwrap();
    tokio::spawn(cut_at_commit(listener, upstream, deliver));
    let through_proxy = PgDriver.connect(options(proxy_port)).await.unwrap();

    let error = through_proxy
        .execute_in_transaction(&[("INSERT INTO ledger VALUES (1)".to_string(), Vec::new())])
        .await
        .unwrap_err();
    let bookie_core::DriverError::OperationOutcomeUnknown { source } = error else {
        panic!("a lost COMMIT must be an unknown outcome");
    };
    let bookie_core::DriverError::CommitUnconfirmed(token) = *source else {
        panic!("the unknown outcome must carry the commit token");
    };
    let resolution = settle(direct.as_ref(), &token).await;
    let rows = direct.query("SELECT count(*) FROM ledger").await.unwrap();
    let count = match rows.rows.iter().next().map(|row| row[0].clone()) {
        Some(bookie_core::Value::Int(count)) => count,
        other => panic!("{other:?}"),
    };
    (resolution, count)
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_commit_that_reached_the_server_resolves_as_applied() {
    assert_eq!(lose_the_commit(true).await, (CommitResolution::Applied, 1));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_commit_that_never_arrived_resolves_as_not_applied() {
    assert_eq!(lose_the_commit(false).await, (CommitResolution::NotApplied, 0));
}
