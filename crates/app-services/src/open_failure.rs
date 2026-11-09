use bookie_storage::KeyringFailure;
use bookie_transport::TransportError;

use crate::driver_error_presentation::{DriverFailurePresentation, classify};

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
