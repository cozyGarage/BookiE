use bookie_core::DriverError;

pub use crate::tls_failure::TlsFailureKind;
use crate::tls_failure::classify_tls_failure;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverFailurePresentation {
    ConnectionRefused,
    DatabaseFileNotFound(String),
    AuthFailed,
    Tls {
        detail: String,
        kind: TlsFailureKind,
    },
    Query {
        message: String,
        sqlstate: Option<String>,
        position: Option<usize>,
    },
    Disconnected,
    ReadOnly,
    PolicyDenied(String),
    Unsupported(String),
    Internal(String),
    Cancelled,
    TimedOut,
    ConcurrentModification,
    OperationOutcomeUnknown(Box<Self>),
    IntegratedAuth(String),
    Save(SaveFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveFailure {
    RolledBack {
        statement_index: usize,
        source: Box<DriverFailurePresentation>,
    },
    PartiallyApplied {
        statement_index: usize,
        source: Box<DriverFailurePresentation>,
    },
    RollbackFailed {
        statement_index: usize,
        source: Box<DriverFailurePresentation>,
        rollback_error: Box<DriverFailurePresentation>,
    },
}

pub fn classify(error: &DriverError) -> DriverFailurePresentation {
    match error {
        DriverError::ConnectionRefused => DriverFailurePresentation::ConnectionRefused,
        DriverError::DatabaseFileNotFound(path) => DriverFailurePresentation::DatabaseFileNotFound(path.clone()),
        DriverError::AuthFailed => DriverFailurePresentation::AuthFailed,
        DriverError::Tls(detail) => DriverFailurePresentation::Tls {
            detail: detail.clone(),
            kind: classify_tls_failure(detail),
        },
        DriverError::Query {
            message,
            sqlstate,
            position,
        } => DriverFailurePresentation::Query {
            message: message.clone(),
            sqlstate: sqlstate.clone(),
            position: *position,
        },
        DriverError::Disconnected | DriverError::CommitUnconfirmed(_) => DriverFailurePresentation::Disconnected,
        DriverError::ReadOnly => DriverFailurePresentation::ReadOnly,
        DriverError::PolicyDenied(detail) => DriverFailurePresentation::PolicyDenied(detail.clone()),
        DriverError::Unsupported(detail) => DriverFailurePresentation::Unsupported(detail.clone()),
        DriverError::Internal(detail) => DriverFailurePresentation::Internal(detail.clone()),
        DriverError::Cancelled => DriverFailurePresentation::Cancelled,
        DriverError::TimedOut => DriverFailurePresentation::TimedOut,
        DriverError::ConcurrentModification => DriverFailurePresentation::ConcurrentModification,
        DriverError::OperationOutcomeUnknown { source } => {
            DriverFailurePresentation::OperationOutcomeUnknown(Box::new(classify(source)))
        }
        DriverError::IntegratedAuth(detail) => DriverFailurePresentation::IntegratedAuth(detail.clone()),
        DriverError::Transaction {
            statement_index: 0,
            source,
        } if matches!(source.as_ref(), DriverError::ConcurrentModification) => classify(source),
        DriverError::Transaction {
            statement_index,
            source,
        } => DriverFailurePresentation::Save(SaveFailure::RolledBack {
            statement_index: *statement_index,
            source: Box::new(classify(source)),
        }),
        DriverError::PartiallyApplied {
            statement_index,
            source,
        } => DriverFailurePresentation::Save(SaveFailure::PartiallyApplied {
            statement_index: *statement_index,
            source: Box::new(classify(source)),
        }),
        DriverError::TransactionRollbackFailed {
            statement_index,
            source,
            rollback_error,
        } => DriverFailurePresentation::Save(SaveFailure::RollbackFailed {
            statement_index: *statement_index,
            source: Box::new(classify(source)),
            rollback_error: Box::new(classify(rollback_error)),
        }),
    }
}

#[cfg(test)]
#[path = "driver_error_presentation_tests.rs"]
mod tests;
