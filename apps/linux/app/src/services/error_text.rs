use tablepro_app_services::driver_error_presentation::{DriverFailurePresentation, SaveFailure, TlsAdvice};
use tablepro_core::DriverError;
use tablepro_core::sql_dialect::BuildSqlError;

pub fn build_sql_message(error: &BuildSqlError) -> String {
    match error {
        BuildSqlError::NoPrimaryKey => crate::tr!("This table has no primary key. Use the modal Edit dialog instead."),
        BuildSqlError::NothingToUpdate => crate::tr!("No changes to save."),
        BuildSqlError::LengthMismatch { expected, got } => {
            crate::tr!("Internal column count mismatch (expected {expected}, got {got}).")
                .replace("{expected}", &expected.to_string())
                .replace("{got}", &got.to_string())
        }
        BuildSqlError::UnrepresentableValue { column } => {
            crate::tr!("Column {column} contains a value that cannot be copied into a SQL statement.")
                .replace("{column}", column)
        }
        BuildSqlError::UnsupportedDriver(driver) => {
            crate::tr!("Optimistic grid updates are not supported for {driver}.").replace("{driver}", driver)
        }
        BuildSqlError::StaleColumns => crate::tr!(
            "This table's columns changed after you started editing (for example, in a Structure tab). \
             Reload the page and reapply your changes."
        ),
    }
}

pub fn driver_message(error: &DriverError) -> String {
    let presentation = tablepro_app_services::driver_error_presentation::classify(error);
    driver_presentation_message(&presentation)
}

fn driver_presentation_message(presentation: &DriverFailurePresentation) -> String {
    match presentation {
        DriverFailurePresentation::ConnectionRefused => crate::tr!("Could not reach the database. Is it running?"),
        DriverFailurePresentation::AuthFailed => crate::tr!("Username or password is wrong."),
        DriverFailurePresentation::Tls {
            detail,
            advice: TlsAdvice::ServerDoesNotSupportTls,
        } => crate::tr!(
            "TLS handshake failed: {detail}. The server does not appear to use TLS. For a local container, set TLS to Disabled."
        )
        .replace("{detail}", detail),
        DriverFailurePresentation::Tls { detail, .. } => crate::tr!(
            "TLS handshake failed: {detail}. Check the server certificate, the certificate authority file and the TLS mode. Do not turn TLS off to get past a certificate error."
        )
        .replace("{detail}", detail),
        DriverFailurePresentation::Query {
            message,
            sqlstate: Some(sqlstate),
            ..
        } => crate::tr!("Query failed (SQLSTATE {sqlstate}): {message}")
            .replace("{sqlstate}", sqlstate)
            .replace("{message}", message),
        DriverFailurePresentation::Query { message, .. } => {
            crate::tr!("Query failed: {message}").replace("{message}", message)
        }
        DriverFailurePresentation::Disconnected => crate::tr!("The connection was closed, or all connections are busy with other queries. Try reconnecting."),
        DriverFailurePresentation::ConcurrentModification => crate::tr!(
            "This row changed in the database after you loaded it. Your edits are still available; reload the row and reapply them."
        ),
        DriverFailurePresentation::ReadOnly => {
            crate::tr!("This connection is read-only. Reopen it without read-only mode to make changes.")
        }
        DriverFailurePresentation::PolicyDenied(detail) => {
            crate::tr!("Blocked by policy: {detail}").replace("{detail}", detail)
        }
        DriverFailurePresentation::Unsupported(detail) => {
            crate::tr!("This driver does not support: {detail}").replace("{detail}", detail)
        }
        DriverFailurePresentation::Internal(detail) => {
            crate::tr!("Internal driver error: {detail}").replace("{detail}", detail)
        }
        DriverFailurePresentation::Cancelled => crate::tr!("The operation was cancelled."),
        DriverFailurePresentation::TimedOut => crate::tr!("The operation timed out."),
        DriverFailurePresentation::OperationOutcomeUnknown(source) => crate::tr!(
            "The operation was interrupted, but the database outcome could not be confirmed: {detail}"
        )
        .replace("{detail}", &driver_presentation_message(source)),
        DriverFailurePresentation::IntegratedAuth(detail) => crate::tr!(
            "Kerberos login failed: {detail}. Check that klist shows a valid ticket, run kinit if it does not, and make sure the server's SPN matches the host you typed."
        )
        .replace("{detail}", detail),
        DriverFailurePresentation::Save(failure) => save_failure_message(failure),
    }
}

fn save_failure_message(failure: &SaveFailure) -> String {
    match failure {
        SaveFailure::RolledBack {
            statement_index,
            source,
        } => {
            crate::tr!("Save failed at statement {n}: {error}. The rollback request succeeded; session state and other non-transactional side effects may remain.")
                .replace("{n}", &statement_index.saturating_add(1).to_string())
                .replace("{error}", driver_presentation_message(source).trim_end_matches('.'))
        }
        SaveFailure::PartiallyApplied {
            statement_index,
            source,
        } => crate::tr!(
            "Save failed at statement {n}: {error}. This database cannot roll a save back, so the statements before it were applied. Refresh the table before saving again."
        )
        .replace("{n}", &statement_index.saturating_add(1).to_string())
        .replace("{error}", driver_presentation_message(source).trim_end_matches('.')),
        SaveFailure::RollbackFailed {
            statement_index,
            source,
            rollback_error,
        } => crate::tr!(
            "Save failed at statement {n}: {error}. Rollback also failed: {rollback_error}. Database changes may have been applied."
        )
        .replace("{n}", &statement_index.saturating_add(1).to_string())
        .replace("{error}", driver_presentation_message(source).trim_end_matches('.'))
        .replace(
            "{rollback_error}",
            driver_presentation_message(rollback_error).trim_end_matches('.'),
        ),
    }
}

pub fn keyring_message(failure: &tablepro_storage::KeyringFailure) -> String {
    use tablepro_storage::KeyringFailure;
    match failure {
        KeyringFailure::Unavailable => crate::tr!(
            "No keyring is running, so saved passwords can't be read. Start GNOME Keyring, KeePassXC with Secret Service, or another Secret Service provider, then try again."
        ),
        KeyringFailure::Locked => crate::tr!(
            "The keyring is locked. Unlock it, for example by signing in again or opening Passwords and Keys, then try again."
        ),
        KeyringFailure::UnlockCancelled => {
            crate::tr!(
                "Unlocking the keyring was cancelled, so the saved password was not read. Try again and unlock it when asked."
            )
        }
        KeyringFailure::Other(detail) => {
            crate::tr!("The keyring could not be used: {detail}").replace("{detail}", detail)
        }
    }
}

#[cfg(test)]
#[path = "error_text_tests.rs"]
mod tests;
