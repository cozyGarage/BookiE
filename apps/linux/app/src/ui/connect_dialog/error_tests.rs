use super::*;
use async_trait::async_trait;
use tablepro_core::{DatabaseDriver, DriverError};

struct CertificateFailure;

#[async_trait]
impl DatabaseDriver for CertificateFailure {
    fn id(&self) -> &'static str {
        "certificate-failure"
    }

    fn display_name(&self) -> &'static str {
        "Certificate failure"
    }

    fn default_port(&self) -> u16 {
        5432
    }

    async fn connect(&self, _options: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        Err(DriverError::Tls("unknown certificate authority".into()))
    }
}

#[tokio::test]
async fn connecting_with_a_bad_certificate_keeps_the_certificate_recovery_guidance() {
    let service = crate::services::database_service::DatabaseService::new_isolated();
    let factory = service.candidate_guard_factory();
    let metadata = crate::services::database_service::ConnectionMetadata {
        id: Uuid::new_v4(),
        name: "Test".into(),
        driver_id: CertificateFailure.id().into(),
        environment: Environment::Local,
        read_only: false,
        server_version: None,
        query_timeout_secs: None,
    };
    let environment = tablepro_transport::SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Refuse);
    let failure = open_candidate(
        &CertificateFailure,
        ConnectOptions::default(),
        None,
        &environment,
        5,
        tokio_util::sync::CancellationToken::new(),
        CandidateBootstrap {
            guard_factory: &factory,
            metadata: &metadata,
        },
    )
    .await
    .err()
    .expect("the test driver refuses the certificate");
    assert!(failure.contains("unknown certificate authority"));
    assert!(failure.contains("certificate authority file"));
    assert!(failure.contains("Do not turn TLS off"));
}
