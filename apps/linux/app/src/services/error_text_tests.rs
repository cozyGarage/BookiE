#[test]
fn a_certificate_failure_never_advises_turning_tls_off() {
    for detail in [
        "certificate verify failed",
        "unknown issuer",
        "certificate hostname mismatch",
    ] {
        let message = driver_message(&DriverError::Tls(detail.into()));
        assert!(message.contains(detail));
        assert!(!message.contains("set TLS to Disabled"), "{detail}: {message}");
        assert!(message.contains("certificate authority file"));
    }
}

#[test]
fn only_a_server_without_tls_suggests_disabling_it() {
    let message = driver_message(&DriverError::Tls("server does not support TLS".into()));
    assert!(message.contains("set TLS to Disabled"));
}

#[test]
fn each_keyring_failure_says_what_to_do_next() {
    use tablepro_storage::KeyringFailure;
    let texts: Vec<String> = [
        KeyringFailure::Unavailable,
        KeyringFailure::Locked,
        KeyringFailure::UnlockCancelled,
        KeyringFailure::Other("boom".into()),
    ]
    .iter()
    .map(keyring_message)
    .collect();
    assert!(texts[0].contains("Start"));
    assert!(texts[1].contains("Unlock"));
    assert!(texts[2].contains("Try again"));
    assert!(texts[3].contains("boom"));
}

use super::*;

#[test]
fn a_first_statement_conflict_keeps_the_recovery_advice_without_a_save_wrapper() {
    let message = driver_message(&DriverError::Transaction {
        statement_index: 0,
        source: Box::new(DriverError::ConcurrentModification),
    });

    assert_eq!(message, driver_message(&DriverError::ConcurrentModification));
}

#[test]
fn a_later_conflict_keeps_the_transaction_side_effect_warning() {
    let message = driver_message(&DriverError::Transaction {
        statement_index: 1,
        source: Box::new(DriverError::ConcurrentModification),
    });

    assert!(message.contains("statement 2"));
    assert!(message.contains("non-transactional side effects may remain"));
}

#[test]
fn save_error_wrappers_do_not_duplicate_sentence_endings() {
    let errors = [
        DriverError::Transaction {
            statement_index: 1,
            source: Box::new(DriverError::ReadOnly),
        },
        DriverError::PartiallyApplied {
            statement_index: 1,
            source: Box::new(DriverError::ConcurrentModification),
        },
        DriverError::TransactionRollbackFailed {
            statement_index: 0,
            source: Box::new(DriverError::ConcurrentModification),
            rollback_error: Box::new(DriverError::TimedOut),
        },
    ];

    for error in errors {
        let message = driver_message(&error);
        assert!(!message.contains(".."), "{message}");
    }
}

#[test]
fn build_sql_messages_have_actionable_advice() {
    let nopk = build_sql_message(&BuildSqlError::NoPrimaryKey);
    assert!(nopk.contains("Edit dialog"));
    let nothing = build_sql_message(&BuildSqlError::NothingToUpdate);
    assert!(nothing.contains("No changes"));
    let mismatch = build_sql_message(&BuildSqlError::LengthMismatch { expected: 3, got: 2 });
    assert!(mismatch.contains("expected 3"));
    assert!(mismatch.contains("got 2"));
}

#[test]
fn driver_messages_include_sqlstate_when_present() {
    let with_state = driver_message(&DriverError::Query {
        message: "duplicate key".into(),
        sqlstate: Some("23505".into()),
        position: None,
    });
    assert!(with_state.contains("23505"));
    let without = driver_message(&DriverError::Query {
        message: "syntax error".into(),
        sqlstate: None,
        position: None,
    });
    assert!(!without.contains("SQLSTATE"));
    assert!(without.contains("syntax error"));
}

#[test]
fn driver_message_for_simple_variants() {
    assert!(driver_message(&DriverError::ConnectionRefused).contains("Could not reach"));
    assert!(driver_message(&DriverError::AuthFailed).contains("wrong"));
    assert!(driver_message(&DriverError::Disconnected).contains("Try reconnecting"));
    assert!(driver_message(&DriverError::Disconnected).contains("busy"));
}

#[test]
fn failed_transaction_rollback_does_not_claim_no_changes() {
    let message = driver_message(&DriverError::TransactionRollbackFailed {
        statement_index: 2,
        source: Box::new(DriverError::Query {
            message: "statement failed".into(),
            sqlstate: None,
            position: None,
        }),
        rollback_error: Box::new(DriverError::Disconnected),
    });
    assert!(message.contains("statement 3"));
    assert!(message.contains("Rollback also failed"));
    assert!(message.contains("Database changes may have been applied"));
    assert!(!message.contains("no rows were changed"));
}

#[test]
fn a_partially_applied_save_says_earlier_statements_stayed_applied() {
    let message = driver_message(&DriverError::non_atomic_batch_failure(
        2,
        DriverError::query("statement failed"),
    ));
    assert!(message.contains("statement 3"));
    assert!(message.contains("before it were applied"));
    assert!(!message.contains("rollback request succeeded"));
}

#[test]
fn confirmed_rollback_warns_that_nontransactional_side_effects_may_remain() {
    let message = driver_message(&DriverError::Transaction {
        statement_index: 0,
        source: Box::new(DriverError::Query {
            message: "statement failed".into(),
            sqlstate: None,
            position: None,
        }),
    });
    assert!(message.contains("rollback request succeeded"));
    assert!(message.contains("session state and other non-transactional side effects may remain"));
    assert!(!message.contains("no rows were changed"));
}

#[test]
fn integrated_auth_names_the_remedy_and_keeps_the_detail() {
    let message = driver_message(&DriverError::IntegratedAuth("No Kerberos credentials available".into()));
    assert!(message.contains("No Kerberos credentials available"));
    assert!(message.contains("kinit"));
    assert!(message.contains("SPN"));
}
