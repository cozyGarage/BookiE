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
