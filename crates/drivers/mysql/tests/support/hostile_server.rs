#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use drivers_mysql::MysqlDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

fn handshake() -> Vec<u8> {
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
    payload.extend_from_slice(b"mysql_native_password\0");
    payload
}

async fn serve_one_packet(payload: Vec<u8>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut frame = payload.len().to_le_bytes()[..3].to_vec();
        frame.push(0);
        frame.extend_from_slice(&payload);
        socket.write_all(&frame).await.unwrap();
        socket.flush().await.unwrap();
    });
    port
}

#[tokio::test]
async fn a_truncated_server_handshake_is_refused_promptly() {
    let full = handshake();
    for length in 0..full.len() {
        let port = serve_one_packet(full[..length].to_vec()).await;
        let options = ConnectOptions {
            host: "127.0.0.1".into(),
            port,
            username: "user".into(),
            database: "db".into(),
            ..Default::default()
        };
        let started = Instant::now();
        let outcome = MysqlDriver.connect(options).await;
        assert!(outcome.is_err(), "a handshake cut to {length} bytes must be refused");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "a handshake cut to {length} bytes was not refused promptly"
        );
    }
}

#[tokio::test]
async fn a_truncated_column_definition_is_reported_as_an_error() {
    use sqlx::{
        Connection,
        mysql::{MySqlConnectOptions, MySqlConnection, MySqlSslMode},
    };
    use tokio::io::AsyncReadExt;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        async fn send(socket: &mut tokio::net::TcpStream, seq: u8, payload: &[u8]) {
            let mut frame = payload.len().to_le_bytes()[..3].to_vec();
            frame.push(seq);
            frame.extend_from_slice(payload);
            socket.write_all(&frame).await.unwrap();
        }
        async fn receive(socket: &mut tokio::net::TcpStream) {
            let mut header = [0; 4];
            socket.read_exact(&mut header).await.unwrap();
            let len = usize::from(header[0]) | (usize::from(header[1]) << 8) | (usize::from(header[2]) << 16);
            let mut payload = vec![0; len];
            socket.read_exact(&mut payload).await.unwrap();
        }
        send(&mut socket, 0, &handshake()).await;
        receive(&mut socket).await;
        send(&mut socket, 2, &[0, 0, 0, 2, 0, 0, 0]).await;
        receive(&mut socket).await;
        send(&mut socket, 1, &[1]).await;
        send(&mut socket, 2, &[0, 0, 0, 0, 0, 0, 12]).await;
    });
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username("user")
        .ssl_mode(MySqlSslMode::Disabled);
    let result = tokio::time::timeout(Duration::from_secs(2), MySqlConnection::connect_with(&options))
        .await
        .unwrap();
    assert!(matches!(result, Err(sqlx::Error::Protocol(_))));
    server.await.unwrap();
}
