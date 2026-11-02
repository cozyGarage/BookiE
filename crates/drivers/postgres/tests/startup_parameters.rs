#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use bookie_core::{ConnectOptions, DatabaseDriver, TlsConfig};
use drivers_postgres::PgDriver;
use tokio::io::AsyncReadExt;

async fn startup_parameters() -> Vec<String> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let attempt = tokio::spawn(PgDriver.connect(ConnectOptions {
        host: "127.0.0.1".into(),
        port,
        tls: TlsConfig::disabled(),
        ..Default::default()
    }));
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut length = [0u8; 4];
    socket.read_exact(&mut length).await.unwrap();
    let mut body = vec![0u8; u32::from_be_bytes(length) as usize - 4];
    socket.read_exact(&mut body).await.unwrap();
    attempt.abort();
    body[4..]
        .split(|byte| *byte == 0)
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

#[tokio::test]
async fn the_startup_message_leaves_the_time_zone_to_the_server_and_role() {
    let parameters = startup_parameters().await;

    assert!(parameters.iter().any(|name| name == "DateStyle"), "{parameters:?}");
    assert!(
        parameters.iter().any(|name| name == "client_encoding"),
        "{parameters:?}"
    );
    assert!(
        !parameters.iter().any(|name| name.eq_ignore_ascii_case("TimeZone")),
        "the driver must not override the server or role time zone: {parameters:?}"
    );
}
