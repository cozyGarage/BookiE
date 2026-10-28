use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tablepro_core::{ConnectOptions, Connection, DriverError};

pub async fn a_ping_does_not_report_a_disconnect_while_every_pooled_connection_is_busy<
    Guard,
    Start,
    Started,
    Connect,
    Connected,
>(
    start: Start,
    connect: Connect,
    long_read: &'static str,
) where
    Start: FnOnce() -> Started,
    Started: Future<Output = (Guard, ConnectOptions)>,
    Connect: FnOnce(ConnectOptions) -> Connected,
    Connected: Future<Output = Box<dyn Connection>>,
{
    let (_server, options) = start().await;
    let connection: Arc<dyn Connection> = Arc::from(connect(options).await);
    let long_reads: Vec<_> = (0..4)
        .map(|_| {
            let connection = Arc::clone(&connection);
            tokio::spawn(async move { connection.query(long_read).await })
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(800)).await;

    let started = Instant::now();
    let outcome = connection.ping().await;

    assert!(outcome.is_ok(), "a busy pool was reported as {outcome:?}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the ping waited {:?} for a connection that cannot free up",
        started.elapsed()
    );
    for read in long_reads {
        read.abort();
    }
}

pub async fn a_ping_still_reports_a_server_that_cannot_be_reached<Guard, Start, Started, Connect, Connected>(
    start: Start,
    connect: Connect,
) where
    Start: FnOnce() -> Started,
    Started: Future<Output = (Guard, ConnectOptions)>,
    Connect: FnOnce(ConnectOptions) -> Connected,
    Connected: Future<Output = Box<dyn Connection>>,
{
    let (server, options) = start().await;
    let connection = connect(options).await;
    let before = connection.ping().await;
    assert!(
        before.is_ok(),
        "the server was not reachable before it was stopped: {before:?}"
    );
    drop(server);

    let outcome = tokio::time::timeout(Duration::from_secs(20), connection.ping()).await;

    assert!(
        matches!(
            outcome,
            Ok(Err(DriverError::Disconnected | DriverError::ConnectionRefused))
        ),
        "an unreachable server was reported as {outcome:?}"
    );
}
