#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Counts the network round trips a statement costs, for #3738. A TCP proxy sits between the driver and a
//! PostgreSQL container and counts bursts: one client-to-server burst after server-to-client traffic is one
//! round trip. Run with `--ignored --nocapture`; it needs Docker and prints the counts.

use std::sync::{Arc, Mutex};

use bookie_core::{ConnectOptions, DatabaseDriver, OperationControl, TlsConfig};
use drivers_postgres::PgDriver;
use secrecy::SecretString;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Default)]
struct Trips {
    last_was_client: bool,
    bursts: u64,
}

struct CountingProxy {
    port: u16,
    trips: Arc<Mutex<Trips>>,
    trace: Arc<std::sync::atomic::AtomicBool>,
}

impl CountingProxy {
    async fn start(upstream_port: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let trips = Arc::new(Mutex::new(Trips::default()));
        let trace = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shared = trips.clone();
        let tracing = trace.clone();
        tokio::spawn(async move {
            loop {
                let Ok((client, _)) = listener.accept().await else {
                    return;
                };
                let server = TcpStream::connect(("127.0.0.1", upstream_port)).await.unwrap();
                let (mut client_read, mut client_write) = client.into_split();
                let (mut server_read, mut server_write) = server.into_split();
                let up = shared.clone();
                let tracing = tracing.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 65_536];
                    while let Ok(read) = client_read.read(&mut buffer).await {
                        if read == 0 {
                            return;
                        }
                        {
                            let mut trips = up.lock().unwrap();
                            if !trips.last_was_client {
                                trips.bursts += 1;
                                if tracing.load(std::sync::atomic::Ordering::Relaxed) {
                                    let text: String = buffer[..read]
                                        .iter()
                                        .map(|byte| {
                                            if byte.is_ascii_graphic() || *byte == b' ' {
                                                *byte as char
                                            } else {
                                                '.'
                                            }
                                        })
                                        .take(90)
                                        .collect();
                                    println!("  burst {}: {text}", trips.bursts);
                                }
                            }
                            trips.last_was_client = true;
                        }
                        if server_write.write_all(&buffer[..read]).await.is_err() {
                            return;
                        }
                    }
                });
                let down = shared.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 65_536];
                    while let Ok(read) = server_read.read(&mut buffer).await {
                        if read == 0 {
                            return;
                        }
                        down.lock().unwrap().last_was_client = false;
                        if client_write.write_all(&buffer[..read]).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });
        Self { port, trips, trace }
    }

    fn trace(&self, on: bool) {
        self.trace.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    fn reset(&self) {
        self.trips.lock().unwrap().bursts = 0;
    }

    fn bursts(&self) -> u64 {
        self.trips.lock().unwrap().bursts
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires docker; prints the round trips per statement measured for #3738"]
async fn the_round_trips_each_statement_costs_are_counted() {
    const RUNS: u64 = 20;
    let container = Postgres::default().with_tag("16-alpine").start().await.unwrap();
    let proxy = CountingProxy::start(container.get_host_port_ipv4(5432).await.unwrap()).await;
    let connection = PgDriver
        .connect(ConnectOptions {
            host: "127.0.0.1".into(),
            port: proxy.port,
            database: "postgres".into(),
            username: "postgres".into(),
            password: SecretString::from("postgres"),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE trips (id integer PRIMARY KEY, name text)")
        .await
        .unwrap();
    for _ in 0..3 {
        connection.query("SELECT 1").await.unwrap();
        connection.query("SELECT id, name FROM trips").await.unwrap();
    }

    let per_statement =
        |label: &str, bursts: u64| println!("round trips per {label}: {:.2}", bursts as f64 / RUNS as f64);

    if std::env::var_os("ROUND_TRIP_TRACE").is_some() {
        proxy.reset();
        proxy.trace(true);
        for round in 1..=3 {
            println!("SELECT 1, run {round}:");
            connection.query("SELECT 1").await.unwrap();
        }
        println!("table SELECT:");
        connection.query("SELECT id, name FROM trips").await.unwrap();
        proxy.trace(false);
    }

    proxy.reset();
    for _ in 0..RUNS {
        connection.query("SELECT 1").await.unwrap();
    }
    let plain = per_statement("SELECT 1 (query)", proxy.bursts());

    proxy.reset();
    for _ in 0..RUNS {
        let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
        connection.query_controlled("SELECT 1", &control).await.unwrap();
    }
    let controlled = per_statement("SELECT 1 (query_controlled)", proxy.bursts());

    proxy.reset();
    for _ in 0..RUNS {
        connection.query("SELECT id, name FROM trips").await.unwrap();
    }
    let table = per_statement("table SELECT (query)", proxy.bursts());
    assert!(
        plain < 4.0 && controlled < 4.3,
        "a plain statement was 4.25 to 4.75 round trips before the ping skip"
    );
    assert!(
        table < 6.5,
        "a table statement was 7.4 round trips before the domain lookup shrank"
    );
}
