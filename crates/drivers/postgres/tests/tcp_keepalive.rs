#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use drivers_postgres::PgDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver};

fn keepalive_armed_towards(port: u16) -> bool {
    let table = std::fs::read_to_string("/proc/self/net/tcp").unwrap();
    table.lines().skip(1).any(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let remote_port = fields[2].rsplit(':').next().unwrap();
        let timer = fields[5].split(':').next().unwrap();
        u16::from_str_radix(remote_port, 16) == Ok(port) && timer == "02"
    })
}

#[tokio::test]
async fn a_direct_connection_has_tcp_keepalive_armed() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let _silent_server = tokio::spawn(async move {
        let (_held, _) = listener.accept().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    });
    let attempt = tokio::spawn(PgDriver.connect(ConnectOptions {
        host: "127.0.0.1".into(),
        port,
        ..Default::default()
    }));

    let mut armed = false;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if keepalive_armed_towards(port) {
            armed = true;
            break;
        }
    }
    attempt.abort();

    assert!(armed, "the client socket to the database must have a keepalive timer");
}
