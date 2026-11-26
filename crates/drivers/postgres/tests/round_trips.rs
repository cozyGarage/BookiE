#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Counts the network round trips a statement costs, for #3738. A TCP proxy sits between the driver and a
//! PostgreSQL container and counts bursts: one client-to-server burst after server-to-client traffic is one
//! round trip. Run with `--ignored --nocapture`; it needs Docker and prints the counts.

#[path = "../../shared/tests/counting_proxy.rs"]
mod counting_proxy;

use bookie_core::{ConnectOptions, DatabaseDriver, OperationControl, TlsConfig};
use counting_proxy::CountingProxy;
use drivers_postgres::PgDriver;
use secrecy::SecretString;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

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

    let per_statement = |label: &str, bursts: u64| {
        let trips = bursts as f64 / RUNS as f64;
        println!("round trips per {label}: {trips:.2}");
        trips
    };

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
        plain < 5.0 && controlled < 5.2,
        "a plain statement is about 4.2 to 4.7 round trips: {plain} {controlled}"
    );
    assert!(
        table < 6.8,
        "a table statement was 7.4 round trips before the domain lookup shrank: {table}"
    );
}
