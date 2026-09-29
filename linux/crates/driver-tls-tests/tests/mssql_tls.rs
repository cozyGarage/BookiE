#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use tablepro_core::{DatabaseDriver, TlsMode};
use tablepro_driver_tls_tests::DriverTlsFixture;

use drivers_mssql::MssqlDriver;

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn a_verifying_mode_connects_with_the_fixture_authority() {
    let fixture = DriverTlsFixture::from_env();
    let connection = MssqlDriver
        .connect(fixture.mssql(TlsMode::VerifyFull, Some(fixture.ca_cert.clone())))
        .await
        .expect("verify full must succeed against the fixture certificate");
    connection.ping().await.expect("a verified session must be usable");
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn verify_ca_accepts_the_fixture_authority() {
    let fixture = DriverTlsFixture::from_env();
    let connection = MssqlDriver
        .connect(fixture.mssql(TlsMode::VerifyCa, Some(fixture.ca_cert.clone())))
        .await
        .expect("verify ca must succeed against a chain the authority signed");
    connection.ping().await.expect("a verified session must be usable");
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn verify_full_rejects_an_ip_endpoint_absent_from_the_certificate() {
    let fixture = DriverTlsFixture::from_env();
    let options = fixture.mssql(TlsMode::VerifyFull, Some(fixture.ca_cert.clone()));
    tablepro_driver_tls_tests::assert_endpoint_identity_rejected(&MssqlDriver, options).await;
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn a_verifying_mode_naming_the_wrong_authority_is_refused() {
    let fixture = DriverTlsFixture::from_env();
    for mode in [TlsMode::VerifyCa, TlsMode::VerifyFull] {
        let accepted = fixture.mssql(mode, Some(fixture.ca_cert.clone()));
        let rejected = fixture.mssql(mode, Some(fixture.other_ca_cert.clone()));
        tablepro_driver_tls_tests::assert_certificate_rejected(&MssqlDriver, accepted, rejected).await;
    }
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn an_encrypt_only_mode_connects_without_an_authority() {
    let fixture = DriverTlsFixture::from_env();
    let connection = MssqlDriver
        .connect(fixture.mssql(TlsMode::Require, None))
        .await
        .expect("encrypt-only must not require an authority it never checks");
    connection.ping().await.expect("an encrypted session must be usable");
}
