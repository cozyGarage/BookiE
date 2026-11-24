use std::time::Duration;

use super::*;

fn saved(json: &str) -> bookie_storage::SavedConnection {
    serde_json::from_str(json).expect("valid saved connection")
}

#[test]
fn a_network_connection_names_its_address_database_tunnel_and_tls_without_secrets() {
    let connection = saved(
        r#"{"id":"6af4b280-82ec-4d81-b780-3878f7af1647","name":"prod","driver_id":"postgres","host":"db.internal","port":5432,"database":"app","username":"admin","use_tls":true,"tls_mode":"verify_ca",
        "ssh":{"host":"bastion","port":22,"username":"tunneluser","auth":{"kind":"password"},"jump":{"host":"edge","port":2222,"username":"tunneluser","auth":{"kind":"password"}}}}"#,
    );
    let details = ConnectionDetails::from_saved(&connection);
    assert_eq!(details.endpoint, "db.internal:5432");
    assert_eq!(details.database, "app");
    assert_eq!(details.ssh_hops, 2);
    assert_eq!(details.tls, "Verify CA");

    let tooltip = details.tooltip(
        Some("PostgreSQL 16.4 on x86_64-pc-linux-gnu"),
        Some(Duration::from_millis(12)),
    );
    assert_eq!(
        tooltip,
        "Server: PostgreSQL 16.4 on x86_64-pc-linux-gnu\nAddress: db.internal:5432\nDatabase: app\nSSH: 2 hops\nTLS: Verify CA\nRound trip: 12 ms"
    );
    for secret in ["admin", "tunneluser", "bastion", "edge"] {
        assert!(!tooltip.contains(secret), "{secret} leaked into {tooltip}");
    }
}

#[test]
fn a_file_or_plain_connection_says_what_it_does_not_use() {
    let file = saved(
        r#"{"id":"6af4b280-82ec-4d81-b780-3878f7af1648","name":"local","driver_id":"sqlite","host":"","port":0,"database":"/tmp/app.db","username":""}"#,
    );
    let details = ConnectionDetails::from_saved(&file);
    assert_eq!(details.endpoint, "");
    assert_eq!(details.tls, "Not used");
    assert_eq!(
        details.tooltip(None, None),
        "Database: /tmp/app.db\nSSH: none\nTLS: Not used"
    );

    let plain = saved(
        r#"{"id":"6af4b280-82ec-4d81-b780-3878f7af1649","name":"dev","driver_id":"mysql","host":"localhost","port":3306,"database":"shop","username":"dev"}"#,
    );
    assert_eq!(ConnectionDetails::from_saved(&plain).tls, "Off");
}

#[test]
fn the_chip_version_is_short_single_line_and_bounded() {
    assert_eq!(
        short_server_version("PostgreSQL 16.4 (Debian 16.4-1.pgdg120+2) on x86_64-pc-linux-gnu, compiled by gcc"),
        "PostgreSQL 16.4"
    );
    assert_eq!(
        short_server_version("8.0.36-0ubuntu0.22.04.1"),
        "8.0.36-0ubuntu0.22.04.1"
    );
    assert_eq!(short_server_version("  3.45.1\nextra"), "3.45.1");
    assert_eq!(short_server_version(&"x".repeat(200)).chars().count(), 32);
    assert_eq!(short_server_version(""), "");
}
