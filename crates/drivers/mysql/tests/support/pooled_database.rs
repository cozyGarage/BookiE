#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{connect, start_mysql};
use tablepro_core::DriverError;

#[tokio::test]
#[ignore = "requires docker"]
async fn use_on_a_pooled_connection_does_not_change_what_later_catalog_reads_see() {
    let (_c, opts) = start_mysql().await;
    let connection = connect(opts.clone()).await;
    connection.execute("CREATE TABLE home_table (id INT)").await.unwrap();
    connection.execute("CREATE DATABASE other_db").await.unwrap();
    connection
        .execute("CREATE TABLE other_db.away_table (id INT)")
        .await
        .unwrap();

    connection
        .query_controlled(
            "USE other_db",
            &tablepro_core::OperationControl::with_timeout(std::time::Duration::from_secs(5)),
        )
        .await
        .unwrap();

    for _ in 0..8 {
        let names: Vec<String> = connection
            .list_tables()
            .await
            .unwrap()
            .into_iter()
            .map(|table| table.name)
            .collect();
        assert_eq!(names, vec!["home_table".to_string()]);
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_health_ping_does_not_report_a_disconnect_while_every_pooled_connection_is_busy() {
    let (_container, opts) = start_mysql().await;
    let connection: Arc<dyn tablepro_core::Connection> = Arc::from(connect(opts).await);
    let long_reads: Vec<_> = (0..4)
        .map(|_| {
            let connection = Arc::clone(&connection);
            tokio::spawn(async move { connection.query("SELECT SLEEP(12)").await })
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

#[tokio::test]
#[ignore = "requires docker"]
async fn a_health_ping_still_reports_a_pool_that_cannot_reach_its_server() {
    let (container, opts) = start_mysql().await;
    let connection = connect(opts).await;
    connection.ping().await.unwrap();
    drop(container);

    let outcome = tokio::time::timeout(Duration::from_secs(20), connection.ping()).await;

    assert!(
        matches!(
            outcome,
            Ok(Err(DriverError::Disconnected | DriverError::ConnectionRefused))
        ),
        "an unreachable server was reported as {outcome:?}"
    );
}
