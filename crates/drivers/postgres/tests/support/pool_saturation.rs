#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::{connect, start_pg};

#[path = "../../../shared/pool_saturation.rs"]
mod contract;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_health_ping_does_not_report_a_disconnect_while_every_pooled_connection_is_busy() {
    contract::a_ping_does_not_report_a_disconnect_while_every_pooled_connection_is_busy(
        start_pg,
        connect,
        "SELECT pg_sleep(12)",
    )
    .await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_health_ping_still_reports_a_pool_that_cannot_reach_its_server() {
    contract::a_ping_still_reports_a_server_that_cannot_be_reached(start_pg, connect).await;
}
