use tablepro_core::{ConnectOptions, DatabaseDriver};

fn armed_towards(port: u16) -> Result<bool, String> {
    let table = std::fs::read_to_string("/proc/self/net/tcp").map_err(|error| error.to_string())?;
    Ok(table.lines().skip(1).any(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let remote_port = fields.get(2).and_then(|field| field.rsplit(':').next());
        let timer = fields.get(5).and_then(|field| field.split(':').next());
        remote_port.and_then(|hex| u16::from_str_radix(hex, 16).ok()) == Some(port) && timer == Some("02")
    }))
}

pub async fn assert_keepalive_armed(driver: &dyn DatabaseDriver) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|error| error.to_string())?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    let _silent_server = tokio::spawn(async move {
        let _held = listener.accept().await;
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    });
    let attempt = driver.connect(ConnectOptions {
        host: "127.0.0.1".into(),
        port,
        ..Default::default()
    });
    let mut attempt = std::pin::pin!(attempt);
    for _ in 0..40 {
        tokio::select! {
            _ = &mut attempt => break,
            () = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
        }
        if armed_towards(port)? {
            return Ok(());
        }
    }
    Err("the client socket to the database has no keepalive timer".into())
}
