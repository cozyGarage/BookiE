#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../shared/keepalive_probe.rs"]
mod keepalive_probe;

#[tokio::test]
async fn a_direct_connection_has_tcp_keepalive_armed() {
    keepalive_probe::assert_keepalive_armed(&drivers_mssql::MssqlDriver)
        .await
        .unwrap();
}
