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
        TlsRemedy::DisableTls => bookie_app_services::tr!("Set TLS to Disabled"),
    }
}

pub(super) fn remedy_applies_to_host(remedy: TlsRemedy, host: &str) -> bool {
    remedy != TlsRemedy::DisableTls || is_loopback_host(host)
}

fn is_loopback_host(host: &str) -> bool {
    let trimmed = host.trim();
    let bare = trimmed
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(trimmed);
    bare.eq_ignore_ascii_case("localhost")
        || bare
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
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
    fn a_server_without_tls_offers_to_disable_it_only_for_loopback_hosts() {
        let failure = ConnectFailure::from_transport(TransportError::Driver(DriverError::Tls(
            "server does not support TLS".into(),
        )));
        let remedy = failure.tls_remedy.expect("a remedy");
        assert_eq!(remedy, TlsRemedy::DisableTls);
        for host in ["localhost", "LOCALHOST", "127.0.0.1", "127.9.9.9", "::1", "[::1]"] {
            assert!(remedy_applies_to_host(remedy, host), "{host}");
        }
        for host in [
            "db.example.com",
            "10.0.0.5",
            "192.168.1.10",
            "",
            "localhost.example.com",
        ] {
            assert!(!remedy_applies_to_host(remedy, host), "{host}");
        }
        assert!(remedy_applies_to_host(TlsRemedy::CheckServerName, "db.example.com"));
    }

    #[test]
    fn other_failures_carry_no_remedy() {
        let failure = ConnectFailure::from_transport(TransportError::Driver(DriverError::AuthFailed));
        assert_eq!(failure.tls_remedy, None);
        assert_eq!(ConnectFailure::from("save failed".to_owned()).tls_remedy, None);
    }
}
