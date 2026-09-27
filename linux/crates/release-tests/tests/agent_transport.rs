#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::Arc;

use tablepro_core::{AuthMode, Connection, TlsMode};
use tablepro_release_tests::Fixture;
use tablepro_ssh::{SshConfig, SshTunnel};
use tablepro_transport::establish;

use drivers_postgres::PgDriver;

fn verifying_options(fixture: &Fixture) -> tablepro_core::ConnectOptions {
    tablepro_core::ConnectOptions {
        host: fixture.database_hostname.clone(),
        port: fixture.database_port,
        database: fixture.database.clone(),
        username: fixture.username.clone(),
        password: secrecy::SecretString::new(fixture.password.clone().into()),
        tls: tablepro_core::TlsConfig {
            mode: TlsMode::VerifyFull,
            root_cert: Some(fixture.ca_cert.clone()),
            ..Default::default()
        },
        auth_mode: AuthMode::Password,
        service_endpoint: None,
        local_socket_dir: None,
        forwarded_socket_dir: None,
        application_name: None,
        connect_timeout_secs: None,
    }
}

fn chain(fixture: &Fixture) -> Vec<SshConfig> {
    vec![fixture.ssh_config()]
}

#[tokio::test]
#[ignore = "requires the postgres release fixture"]
async fn the_shared_transport_verifies_the_database_hostname_through_the_bastion() {
    let fixture = Fixture::from_env();
    let (connection, tunnel) = establish(
        &PgDriver,
        verifying_options(&fixture),
        Some(tablepro_transport::SshRoute::Builtin(chain(&fixture))),
        &tablepro_transport::SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Learn),
    )
    .await
    .expect("a tunnelled VerifyFull session");
    let tunnel = tunnel.expect("an ssh chain must produce a tunnel");
    assert!(
        tunnel.socket_dir().is_some(),
        "a verifying PostgreSQL session must be forwarded over a private unix socket"
    );

    let result = connection
        .query("SELECT count(*) FROM release_items")
        .await
        .expect("query over the tunnelled session");
    assert_eq!(result.rows.len(), 1);
}

#[tokio::test]
#[ignore = "requires the postgres release fixture"]
async fn a_tunnelled_connection_fails_closed_when_the_bastion_is_unreachable() {
    let fixture = Fixture::from_env();
    let mut hop = fixture.ssh_config();
    hop.port = 1;

    let error = establish(
        &PgDriver,
        verifying_options(&fixture),
        Some(tablepro_transport::SshRoute::Builtin(vec![hop])),
        &tablepro_transport::SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Learn),
    )
    .await
    .err()
    .expect("an unreachable bastion must fail the connection");

    assert!(
        error.to_string().starts_with("ssh:"),
        "the failure must come from the tunnel: {error}"
    );
}

#[tokio::test]
#[ignore = "requires the postgres release fixture"]
async fn a_direct_session_and_a_tunnelled_session_reach_the_same_database() {
    let fixture = Fixture::from_env();
    let direct: Arc<dyn Connection> = Arc::from(fixture.connect_verified().await);
    let (tunnelled, _tunnel) = establish(
        &PgDriver,
        verifying_options(&fixture),
        Some(tablepro_transport::SshRoute::Builtin(chain(&fixture))),
        &tablepro_transport::SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Learn),
    )
    .await
    .expect("a tunnelled VerifyFull session");

    direct
        .execute("CREATE TABLE IF NOT EXISTS agent_transport_probe (note text)")
        .await
        .expect("create the probe table");
    direct
        .execute("INSERT INTO agent_transport_probe VALUES ('through the bastion')")
        .await
        .expect("insert a probe row");

    let seen = tunnelled
        .query("SELECT note FROM agent_transport_probe")
        .await
        .expect("read the probe row over the tunnel");
    assert_eq!(seen.rows.len(), 1);

    direct
        .execute("DROP TABLE agent_transport_probe")
        .await
        .expect("drop the probe table");
}

#[tokio::test]
#[ignore = "requires the postgres release fixture"]
async fn agentd_refuses_an_unknown_host_key_with_an_empty_known_hosts_file() {
    let fixture = Fixture::from_env();
    let known_hosts = tablepro_ssh::default_known_hosts_path().expect("a known_hosts path must be resolvable");
    let backup = known_hosts.with_extension("bak-refuse-test");
    if known_hosts.exists() {
        std::fs::rename(&known_hosts, &backup).expect("set aside any already-learned host keys");
    }
    assert!(
        !known_hosts.exists(),
        "the refusal case must start from an empty known_hosts file"
    );

    let result = SshTunnel::open(
        fixture.ssh_config(),
        fixture.database_hostname.clone(),
        fixture.database_port,
        tablepro_ssh::UnknownHostKey::Refuse,
    )
    .await;

    let known_hosts_written = known_hosts.exists();
    if backup.exists() {
        std::fs::rename(&backup, &known_hosts).expect("restore the known_hosts file for later tests");
    }

    let error = result.err().expect("an unknown host key must be refused, not learned");
    assert!(
        matches!(error, tablepro_ssh::SshError::UnknownHostKey { .. }),
        "the refused connection must fail with UnknownHostKey, got: {error}"
    );
    assert!(
        !known_hosts_written,
        "refusing an unknown host key must not write a known_hosts file"
    );
}
