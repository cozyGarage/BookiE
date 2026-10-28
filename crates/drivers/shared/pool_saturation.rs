#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use tablepro_core::{Connection, DriverError};

pub async fn a_ping_does_not_report_a_disconnect_while_every_pooled_connection_is_busy(
    connection: Arc<dyn Connection>,
    long_read: &'static str,
) {
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

pub async fn a_ping_still_reports_a_server_that_cannot_be_reached(
    connection: Box<dyn Connection>,
    stop_server: impl FnOnce(),
) {
    connection.ping().await.unwrap();
    stop_server();

    let outcome = tokio::time::timeout(Duration::from_secs(20), connection.ping()).await;

    assert!(
        matches!(
            outcome,
            Ok(Err(DriverError::Disconnected | DriverError::ConnectionRefused))
        ),
        "an unreachable server was reported as {outcome:?}"
    );
}

macro_rules! pool_saturation_tests {
    ($start:ident, $long_read:expr) => {
        #[tokio::test]
        #[ignore = "requires docker"]
        async fn a_health_ping_does_not_report_a_disconnect_while_every_pooled_connection_is_busy() {
            let (_container, opts) = $start().await;
            let connection = std::sync::Arc::from(connect(opts).await);
            contract::a_ping_does_not_report_a_disconnect_while_every_pooled_connection_is_busy(connection, $long_read)
                .await;
        }

        #[tokio::test]
        #[ignore = "requires docker"]
        async fn a_health_ping_still_reports_a_pool_that_cannot_reach_its_server() {
            let (container, opts) = $start().await;
            let connection = connect(opts).await;
            contract::a_ping_still_reports_a_server_that_cannot_be_reached(connection, || drop(container)).await;
        }
    };
}
