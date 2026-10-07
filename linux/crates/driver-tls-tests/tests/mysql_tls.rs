#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use tablepro_core::{DatabaseDriver, TlsMode};
use tablepro_driver_tls_tests::DriverTlsFixture;

use drivers_mysql::MysqlDriver;

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn a_verifying_mode_connects_with_the_fixture_authority() {
    let fixture = DriverTlsFixture::from_env();
    let connection = MysqlDriver
        .connect(fixture.mysql(TlsMode::VerifyFull, Some(fixture.ca_cert.clone())))
        .await
        .expect("verify full must succeed against the fixture certificate");
    connection.ping().await.expect("a verified session must be usable");
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn verify_ca_accepts_the_fixture_authority() {
    let fixture = DriverTlsFixture::from_env();
    let connection = MysqlDriver
        .connect(fixture.mysql(TlsMode::VerifyCa, Some(fixture.ca_cert.clone())))
        .await
        .expect("verify ca must succeed against a chain the authority signed");
    connection.ping().await.expect("a verified session must be usable");
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn a_plaintext_mode_is_refused_by_a_tls_only_server() {
    let fixture = DriverTlsFixture::from_env();
    let result = MysqlDriver.connect(fixture.mysql(TlsMode::Disabled, None)).await;
    assert!(
        result.is_err(),
        "a server that requires secure transport must refuse an unencrypted client"
    );
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn verify_full_rejects_an_ip_endpoint_absent_from_the_certificate() {
    let fixture = DriverTlsFixture::from_env();
    let options = fixture.mysql(TlsMode::VerifyFull, Some(fixture.ca_cert.clone()));
    tablepro_driver_tls_tests::assert_endpoint_identity_rejected(&MysqlDriver, options).await;
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn a_verifying_mode_without_an_authority_is_refused() {
    let fixture = DriverTlsFixture::from_env();
    for mode in [TlsMode::VerifyCa, TlsMode::VerifyFull] {
        let accepted = fixture.mysql(mode, Some(fixture.ca_cert.clone()));
        let rejected = fixture.mysql(mode, None);
        tablepro_driver_tls_tests::assert_certificate_rejected(&MysqlDriver, accepted, rejected).await;
    }
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn a_verifying_mode_naming_the_wrong_authority_is_refused() {
    let fixture = DriverTlsFixture::from_env();
    for mode in [TlsMode::VerifyCa, TlsMode::VerifyFull] {
        let accepted = fixture.mysql(mode, Some(fixture.ca_cert.clone()));
        let rejected = fixture.mysql(mode, Some(fixture.other_ca_cert.clone()));
        tablepro_driver_tls_tests::assert_certificate_rejected(&MysqlDriver, accepted, rejected).await;
    }
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn an_encrypt_only_mode_connects_without_an_authority() {
    let fixture = DriverTlsFixture::from_env();
    let connection = MysqlDriver
        .connect(fixture.mysql(TlsMode::Require, None))
        .await
        .expect("encrypt-only must not require an authority it never checks");
    connection.ping().await.expect("an encrypted session must be usable");
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn verify_full_through_ssh_reaches_the_unpublished_server_and_runs_a_query() {
    let fixture = DriverTlsFixture::from_env();
    let tunnel = fixture.open_mysql_ssh_tunnel().await;
    let connection = MysqlDriver
        .connect(fixture.mysql_ssh(
            "mysql-ssh.tablepro.test",
            &tunnel,
            TlsMode::VerifyFull,
            Some(fixture.ca_cert.clone()),
        ))
        .await
        .expect("verify full must use the original MySQL service name through SSH");
    let result = connection
        .query("SELECT 1")
        .await
        .expect("native query through verified tunnel");
    assert_eq!(result.rows.len(), 1);
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn verify_full_through_ssh_rejects_an_untrusted_authority() {
    let fixture = DriverTlsFixture::from_env();
    let tunnel = fixture.open_mysql_ssh_tunnel().await;
    let accepted = fixture.mysql_ssh(
        "mysql-ssh.tablepro.test",
        &tunnel,
        TlsMode::VerifyFull,
        Some(fixture.ca_cert.clone()),
    );
    let rejected = fixture.mysql_ssh(
        "mysql-ssh.tablepro.test",
        &tunnel,
        TlsMode::VerifyFull,
        Some(fixture.other_ca_cert.clone()),
    );
    tablepro_driver_tls_tests::assert_certificate_rejected(&MysqlDriver, accepted, rejected).await;
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn verify_full_through_ssh_rejects_a_wrong_or_local_identity() {
    let fixture = DriverTlsFixture::from_env();
    let tunnel = fixture.open_mysql_ssh_tunnel().await;
    let accepted = fixture.mysql_ssh(
        "mysql-ssh.tablepro.test",
        &tunnel,
        TlsMode::VerifyFull,
        Some(fixture.ca_cert.clone()),
    );
    let wrong_host = fixture.mysql_ssh(
        "wrong.tablepro.test",
        &tunnel,
        TlsMode::VerifyFull,
        Some(fixture.ca_cert.clone()),
    );
    tablepro_driver_tls_tests::assert_certificate_rejected(&MysqlDriver, accepted.clone(), wrong_host).await;

    let local_identity = fixture.mysql_ssh("127.0.0.1", &tunnel, TlsMode::VerifyFull, Some(fixture.ca_cert.clone()));
    let error = MysqlDriver
        .connect(local_identity)
        .await
        .err()
        .expect("verify full must not use the local tunnel address as the TLS identity");
    assert!(
        matches!(error, tablepro_core::DriverError::Tls(_)),
        "expected TLS identity rejection, got {error}"
    );
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn a_tls_rejection_does_not_fall_back_to_plaintext_and_dropping_tunnel_cleans_up() {
    let fixture = DriverTlsFixture::from_env();
    let tunnel = fixture.open_mysql_ssh_tunnel().await;
    let socket_dir = tunnel.socket_dir().expect("socket tunnel directory").to_path_buf();
    let rejected = fixture.mysql_ssh("mysql-ssh.tablepro.test", &tunnel, TlsMode::Disabled, None);
    assert!(
        MysqlDriver.connect(rejected).await.is_err(),
        "TLS-only MySQL must reject plaintext over the SSH forward"
    );
    drop(tunnel);
    assert!(
        !socket_dir.exists(),
        "dropping a rejected tunnel must remove its local socket directory"
    );
}
