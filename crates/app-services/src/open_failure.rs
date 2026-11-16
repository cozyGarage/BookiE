use bookie_storage::KeyringFailure;
use bookie_transport::TransportError;

use crate::driver_error_presentation::{DriverFailurePresentation, TlsFailureKind, classify};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenFailure {
    Driver(DriverFailurePresentation),
    Keyring(KeyringFailure),
    IntegratedAuthUnsupported(String),
    PolicyUnavailable,
    Other(String),
}

impl OpenFailure {
    pub fn from_driver(error: &bookie_core::DriverError) -> Self {
        Self::Driver(classify(error))
    }
}

impl OpenFailure {
    pub fn tls_failure_kind(&self) -> Option<TlsFailureKind> {
        match self {
            Self::Driver(DriverFailurePresentation::Tls { kind, .. }) => Some(*kind),
            _ => None,
        }
    }
}

impl From<TransportError> for OpenFailure {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::Driver(error) => Self::from_driver(&error),
            TransportError::Keyring(failure) => Self::Keyring(failure),
            TransportError::IntegratedAuthUnsupported(driver) => Self::IntegratedAuthUnsupported(driver),
            other => Self::Other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::DriverError;

    #[test]
    fn a_tls_failure_keeps_its_kind_and_other_failures_have_none() {
        let tls = OpenFailure::from(TransportError::Driver(DriverError::Tls(
            "invalid peer certificate: UnknownIssuer".into(),
        )));
        assert_eq!(tls.tls_failure_kind(), Some(TlsFailureKind::UnknownIssuer));
        let refused = OpenFailure::from(TransportError::Driver(DriverError::ConnectionRefused));
        assert_eq!(refused.tls_failure_kind(), None);
    }

    #[test]
    fn transport_failures_keep_their_kind_for_the_frontend_to_word() {
        assert_eq!(
            OpenFailure::from(TransportError::Keyring(KeyringFailure::Locked)),
            OpenFailure::Keyring(KeyringFailure::Locked)
        );
        assert_eq!(
            OpenFailure::from(TransportError::IntegratedAuthUnsupported("sqlite".into())),
            OpenFailure::IntegratedAuthUnsupported("sqlite".into())
        );
        assert!(matches!(
            OpenFailure::from(TransportError::Driver(DriverError::Cancelled)),
            OpenFailure::Driver(DriverFailurePresentation::Cancelled)
        ));
    }
}
