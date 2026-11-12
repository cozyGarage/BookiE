use bookie_app_services::open_failure::OpenFailure;
use bookie_app_services::tls_failure::TlsRemedy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConnectFailure {
    pub message: String,
    pub tls_remedy: Option<TlsRemedy>,
}

impl ConnectFailure {
    pub(super) fn from_transport(error: bookie_transport::TransportError) -> Self {
        let failure = OpenFailure::from(error);
        Self {
            message: crate::services::error_text::open_failure_message(&failure),
            tls_remedy: failure.tls_failure_kind().and_then(|kind| kind.remedy()),
        }
    }
}

impl From<String> for ConnectFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            tls_remedy: None,
        }
    }
}

pub(super) fn remedy_label(remedy: TlsRemedy) -> String {
    match remedy {
        TlsRemedy::ChooseCertificateAuthority => bookie_app_services::tr!("Choose certificate authority"),
        TlsRemedy::CheckServerName => bookie_app_services::tr!("Check host name"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::DriverError;
    use bookie_transport::TransportError;

    #[test]
    fn a_trust_failure_carries_the_certificate_authority_remedy() {
        let failure = ConnectFailure::from_transport(TransportError::Driver(DriverError::Tls(
            "invalid peer certificate: UnknownIssuer".into(),
        )));
        assert_eq!(failure.tls_remedy, Some(TlsRemedy::ChooseCertificateAuthority));
        assert!(failure.message.contains("certificate authority"));
    }

    #[test]
    fn a_name_failure_carries_the_host_name_remedy() {
        let failure = ConnectFailure::from_transport(TransportError::Driver(DriverError::Tls(
            "certificate verify failed: hostname mismatch".into(),
        )));
        assert_eq!(failure.tls_remedy, Some(TlsRemedy::CheckServerName));
    }

    #[test]
    fn other_failures_carry_no_remedy() {
        let failure = ConnectFailure::from_transport(TransportError::Driver(DriverError::AuthFailed));
        assert_eq!(failure.tls_remedy, None);
        assert_eq!(ConnectFailure::from("save failed".to_owned()).tls_remedy, None);
    }
}
