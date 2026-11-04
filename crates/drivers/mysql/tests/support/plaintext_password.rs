#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bookie_core::{ConnectOptions, DatabaseDriver, DriverError, TlsConfig, TlsMode};
use drivers_mysql::MysqlDriver;
use secrecy::SecretString;
use testcontainers::ImageExt;
use testcontainers::core::IntoContainerPort;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, UnixListener};

const RSA_PLUGINS: [&str; 2] = ["caching_sha2_password", "sha256_password"];

fn handshake(plugin: &str) -> Vec<u8> {
    let mut payload = vec![10];
    payload.extend_from_slice(b"8.0.36\0");
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&[1; 8]);
    payload.push(0);
    payload.extend_from_slice(&[0x01, 0x82]);
    payload.push(45);
    payload.extend_from_slice(&2u16.to_le_bytes());
    payload.extend_from_slice(&[0x08, 0x00]);
    payload.push(21);
    payload.extend_from_slice(&[0; 10]);
    payload.extend_from_slice(&[2; 13]);
    payload.extend_from_slice(plugin.as_bytes());
    payload.push(0);
    payload
}

async fn send<S: AsyncWrite + Unpin>(socket: &mut S, sequence: u8, payload: &[u8]) {
    let mut frame = payload.len().to_le_bytes()[..3].to_vec();
    frame.push(sequence);
    frame.extend_from_slice(payload);
    socket.write_all(&frame).await.unwrap();
    socket.flush().await.unwrap();
}

async fn receive<S: AsyncRead + Unpin>(socket: &mut S) -> Option<(u8, Vec<u8>)> {
    let mut header = [0; 4];
    socket.read_exact(&mut header).await.ok()?;
    let length = usize::from(header[0]) | (usize::from(header[1]) << 8) | (usize::from(header[2]) << 16);
    let mut payload = vec![0; length];
    socket.read_exact(&mut payload).await.ok()?;
    Some((header[3], payload))
}

async fn converse<S: AsyncRead + AsyncWrite + Unpin>(mut socket: S, plugin: &str, asked_for_key: Arc<AtomicBool>) {
    send(&mut socket, 0, &handshake(plugin)).await;
    while let Some((sequence, payload)) = receive(&mut socket).await {
        if payload.len() == 1 && matches!(payload[0], 1 | 2) {
            asked_for_key.store(true, Ordering::SeqCst);
            return;
        }
        if plugin == "caching_sha2_password" {
            send(&mut socket, sequence.wrapping_add(1), &[1, 4]).await;
        }
    }
}

fn options(mode: TlsMode) -> ConnectOptions {
    ConnectOptions {
        username: "user".into(),
        database: "db".into(),
        password: SecretString::new("never-sent-0b7f".to_string().into()),
        tls: TlsConfig {
            mode,
            ..Default::default()
        },
        ..Default::default()
    }
}

async fn settle(
    options: ConnectOptions,
    server: tokio::task::JoinHandle<()>,
    asked: &AtomicBool,
) -> (Result<(), DriverError>, bool) {
    let outcome = tokio::time::timeout(Duration::from_secs(10), MysqlDriver.connect(options))
        .await
        .expect("the connect attempt must not hang")
        .map(|_| ());
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
    (outcome, asked.load(Ordering::SeqCst))
}

async fn connect_over_tcp(plugin: &'static str, mode: TlsMode) -> (Result<(), DriverError>, bool) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let asked = Arc::new(AtomicBool::new(false));
    let server = tokio::spawn({
        let asked = asked.clone();
        async move {
            let (socket, _) = listener.accept().await.unwrap();
            converse(socket, plugin, asked).await;
        }
    });
    let mut options = options(mode);
    options.host = "127.0.0.1".into();
    options.port = port;
    settle(options, server, &asked).await
}

async fn connect_over_socket(plugin: &'static str) -> (Result<(), DriverError>, bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mysqld.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let asked = Arc::new(AtomicBool::new(false));
    let server = tokio::spawn({
        let asked = asked.clone();
        async move {
            let (socket, _) = listener.accept().await.unwrap();
            converse(socket, plugin, asked).await;
        }
    });
    let mut options = options(TlsMode::Disabled);
    options.local_socket_path = Some(path);
    settle(options, server, &asked).await
}

fn assert_refused_without_sending_a_key_request(outcome: (Result<(), DriverError>, bool), what: &str) {
    let (result, asked) = outcome;
    assert!(
        !asked,
        "{what}: the client asked for the server key over an unprotected connection"
    );
    let Err(DriverError::PolicyDenied(message)) = result else {
        panic!("{what}: expected a policy refusal, got {result:?}");
    };
    assert!(message.contains("without TLS"), "{what}: {message}");
    assert!(
        message.contains("Require") && message.contains("local socket"),
        "{what}: {message}"
    );
    assert!(
        !message.contains("never-sent-0b7f"),
        "{what}: the password leaked into the error"
    );
}

#[tokio::test]
async fn an_rsa_password_exchange_over_tcp_without_tls_is_refused_before_the_key_request() {
    for plugin in RSA_PLUGINS {
        let outcome = connect_over_tcp(plugin, TlsMode::Disabled).await;
        assert_refused_without_sending_a_key_request(outcome, plugin);
    }
}

#[tokio::test]
async fn preferring_tls_does_not_fall_back_to_an_unprotected_rsa_exchange() {
    for plugin in RSA_PLUGINS {
        let outcome = connect_over_tcp(plugin, TlsMode::Prefer).await;
        assert_refused_without_sending_a_key_request(outcome, plugin);
    }
}

#[tokio::test]
async fn a_local_socket_still_takes_the_rsa_exchange() {
    for plugin in RSA_PLUGINS {
        let (_, asked) = connect_over_socket(plugin).await;
        assert!(asked, "{plugin}: a local socket must still request the server key");
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_real_mysql_default_login_needs_tls_over_tcp_and_succeeds_with_it() {
    let image = Mysql::default().with_env_var("MYSQL_ROOT_PASSWORD", "bookie_test");
    let image = match std::env::var("BOOKIE_TEST_RUN_ID") {
        Ok(run_id) => image.with_label("com.bookie.test-run", run_id),
        Err(_) => image,
    };
    let container = image
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("start mysql container");
    let port = container.get_host_port_ipv4(3306.tcp()).await.unwrap();
    let credentials = |mode| {
        let mut options = options(mode);
        options.host = "127.0.0.1".into();
        options.port = port;
        options.username = "root".into();
        options.database = "mysql".into();
        options.password = SecretString::new("bookie_test".to_string().into());
        options
    };
    let refused = MysqlDriver.connect(credentials(TlsMode::Disabled)).await;
    assert!(
        matches!(refused, Err(DriverError::PolicyDenied(_))),
        "{:?}",
        refused.err()
    );
    let allowed = MysqlDriver.connect(credentials(TlsMode::Require)).await;
    assert!(allowed.is_ok(), "{:?}", allowed.err());
}
