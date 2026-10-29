use super::*;

#[test]
fn every_driver_error_variant_has_a_typed_classification() {
    let errors = [
        (
            DriverError::ConnectionRefused,
            DriverFailurePresentation::ConnectionRefused,
        ),
        (DriverError::AuthFailed, DriverFailurePresentation::AuthFailed),
        (
            DriverError::Tls("certificate verify failed".into()),
            DriverFailurePresentation::Tls {
                detail: "certificate verify failed".into(),
                advice: TlsAdvice::VerifyCertificate,
            },
        ),
        (
            DriverError::query("syntax error"),
            DriverFailurePresentation::Query {
                message: "syntax error".into(),
                sqlstate: None,
                position: None,
            },
        ),
        (DriverError::Disconnected, DriverFailurePresentation::Disconnected),
        (DriverError::ReadOnly, DriverFailurePresentation::ReadOnly),
        (
            DriverError::PolicyDenied("read only".into()),
            DriverFailurePresentation::PolicyDenied("read only".into()),
        ),
        (
            DriverError::Unsupported("operation".into()),
            DriverFailurePresentation::Unsupported("operation".into()),
        ),
        (
            DriverError::Internal("failure".into()),
            DriverFailurePresentation::Internal("failure".into()),
        ),
        (DriverError::Cancelled, DriverFailurePresentation::Cancelled),
        (DriverError::TimedOut, DriverFailurePresentation::TimedOut),
        (
            DriverError::ConcurrentModification,
            DriverFailurePresentation::ConcurrentModification,
        ),
        (
            DriverError::OperationOutcomeUnknown {
                source: Box::new(DriverError::TimedOut),
            },
            DriverFailurePresentation::OperationOutcomeUnknown(Box::new(DriverFailurePresentation::TimedOut)),
        ),
        (
            DriverError::IntegratedAuth("no ticket".into()),
            DriverFailurePresentation::IntegratedAuth("no ticket".into()),
        ),
        (
            DriverError::Transaction {
                statement_index: 4,
                source: Box::new(DriverError::ReadOnly),
            },
            DriverFailurePresentation::Save(SaveFailure::RolledBack {
                statement_index: 4,
                source: Box::new(DriverFailurePresentation::ReadOnly),
            }),
        ),
        (
            DriverError::PartiallyApplied {
                statement_index: 5,
                source: Box::new(DriverError::ReadOnly),
            },
            DriverFailurePresentation::Save(SaveFailure::PartiallyApplied {
                statement_index: 5,
                source: Box::new(DriverFailurePresentation::ReadOnly),
            }),
        ),
        (
            DriverError::TransactionRollbackFailed {
                statement_index: 6,
                source: Box::new(DriverError::ReadOnly),
                rollback_error: Box::new(DriverError::TimedOut),
            },
            DriverFailurePresentation::Save(SaveFailure::RollbackFailed {
                statement_index: 6,
                source: Box::new(DriverFailurePresentation::ReadOnly),
                rollback_error: Box::new(DriverFailurePresentation::TimedOut),
            }),
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(classify(&error), expected);
    }
}

#[test]
fn refused_auth_timeout_and_cancel_remain_distinct() {
    for (error, expected) in [
        (
            DriverError::ConnectionRefused,
            DriverFailurePresentation::ConnectionRefused,
        ),
        (DriverError::AuthFailed, DriverFailurePresentation::AuthFailed),
        (DriverError::TimedOut, DriverFailurePresentation::TimedOut),
        (DriverError::Cancelled, DriverFailurePresentation::Cancelled),
    ] {
        assert_eq!(classify(&error), expected);
    }
}

#[test]
fn tls_advice_distinguishes_missing_server_tls_from_verification_failure() {
    let certificate = classify(&DriverError::Tls("certificate hostname mismatch".into()));
    assert_eq!(
        certificate,
        DriverFailurePresentation::Tls {
            detail: "certificate hostname mismatch".into(),
            advice: TlsAdvice::VerifyCertificate,
        }
    );

    let unsupported = classify(&DriverError::Tls("server does not support TLS".into()));
    assert_eq!(
        unsupported,
        DriverFailurePresentation::Tls {
            detail: "server does not support TLS".into(),
            advice: TlsAdvice::ServerDoesNotSupportTls,
        }
    );
}

#[test]
fn unknown_write_outcome_keeps_its_structured_cause() {
    assert_eq!(
        classify(&DriverError::OperationOutcomeUnknown {
            source: Box::new(DriverError::TimedOut),
        }),
        DriverFailurePresentation::OperationOutcomeUnknown(Box::new(DriverFailurePresentation::TimedOut))
    );
}

#[test]
fn save_failure_preserves_statement_and_rollback_state() {
    assert_eq!(
        classify(&DriverError::PartiallyApplied {
            statement_index: 2,
            source: Box::new(DriverError::ReadOnly),
        }),
        DriverFailurePresentation::Save(SaveFailure::PartiallyApplied {
            statement_index: 2,
            source: Box::new(DriverFailurePresentation::ReadOnly),
        })
    );
}

#[test]
fn first_statement_optimistic_conflict_keeps_its_specific_recovery_category() {
    assert_eq!(
        classify(&DriverError::Transaction {
            statement_index: 0,
            source: Box::new(DriverError::ConcurrentModification),
        }),
        DriverFailurePresentation::ConcurrentModification
    );
}

#[test]
fn rollback_failure_keeps_both_causes_separate() {
    assert_eq!(
        classify(&DriverError::TransactionRollbackFailed {
            statement_index: 1,
            source: Box::new(DriverError::ReadOnly),
            rollback_error: Box::new(DriverError::TimedOut),
        }),
        DriverFailurePresentation::Save(SaveFailure::RollbackFailed {
            statement_index: 1,
            source: Box::new(DriverFailurePresentation::ReadOnly),
            rollback_error: Box::new(DriverFailurePresentation::TimedOut),
        })
    );
}

#[test]
fn query_message_and_sqlstate_are_structured_without_a_rendered_sentence() {
    assert_eq!(
        classify(&DriverError::Query {
            message: "permission denied".into(),
            sqlstate: Some("42501".into()),
            position: Some(8),
        }),
        DriverFailurePresentation::Query {
            message: "permission denied".into(),
            sqlstate: Some("42501".into()),
            position: Some(8),
        }
    );
}
