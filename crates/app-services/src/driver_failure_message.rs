use crate::driver_error_presentation::{DriverFailurePresentation, SaveFailure, TlsFailureKind};
use crate::tls_failure::redacted_tls_detail;

pub fn driver_failure_message(presentation: &DriverFailurePresentation) -> String {
    match presentation {
        DriverFailurePresentation::ConnectionRefused => crate::tr!("Could not reach the database. Is it running?"),
        DriverFailurePresentation::DatabaseFileNotFound(path) => crate::tr!(
            "The database file {path} does not exist. Check the path, or use Create a new database file in the connection form."
        )
        .replace("{path}", path),
        DriverFailurePresentation::AuthFailed => crate::tr!("Username or password is wrong."),
        DriverFailurePresentation::Tls { detail, kind } => tls_message(*kind, detail),
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
        DriverFailurePresentation::Disconnected => crate::tr!(
            "The connection was closed, or all connections are busy with other queries. Try reconnecting."
        ),
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
        .replace("{detail}", &driver_failure_message(source)),
        DriverFailurePresentation::IntegratedAuth(detail) => crate::tr!(
            "Kerberos login failed: {detail}. Check that klist shows a valid ticket, run kinit if it does not, and make sure the server's SPN matches the host you typed."
        )
        .replace("{detail}", detail),
        DriverFailurePresentation::Save(failure) => save_failure_message(failure),
    }
}

fn tls_message(kind: TlsFailureKind, detail: &str) -> String {
    let never_off = crate::tr!("Do not turn TLS off or lower verification to get past this.");
    let step = match kind {
        TlsFailureKind::UnknownIssuer => crate::tr!(
            "The server's certificate is not trusted. Choose the certificate authority file that signed it in the TLS section."
        ),
        TlsFailureKind::NameMismatch => crate::tr!(
            "The server's certificate is for a different host name than the one you typed. Check the host name, or ask for a certificate that covers it."
        ),
        TlsFailureKind::Expired => {
            crate::tr!("The server's certificate has expired. Ask the server's owner to renew it.")
        }
        TlsFailureKind::NotYetValid => crate::tr!(
            "The server's certificate is not valid yet. Check that this computer's clock is correct, then ask the server's owner if it still fails."
        ),
        TlsFailureKind::Revoked => {
            crate::tr!("The server's certificate was revoked. Ask the server's owner for a new certificate.")
        }
        TlsFailureKind::ServerRefusedTls => {
            return crate::tr!(
                "TLS handshake failed: {detail}. The server does not appear to use TLS. For a local container, set TLS to Disabled."
            )
            .replace("{detail}", &redacted_tls_detail(detail));
        }
        TlsFailureKind::Other => crate::tr!(
            "TLS handshake failed: {detail}. Check the server certificate, the certificate authority file and the TLS mode."
        )
        .replace("{detail}", &redacted_tls_detail(detail)),
    };
    format!("{step} {never_off}")
}

fn save_failure_message(failure: &SaveFailure) -> String {
    match failure {
        SaveFailure::RolledBack {
            statement_index,
            source,
        } => crate::tr!(
            "Save failed at statement {n}: {error}. The rollback request succeeded; session state and other non-transactional side effects may remain."
        )
        .replace("{n}", &statement_index.saturating_add(1).to_string())
        .replace("{error}", driver_failure_message(source).trim_end_matches('.')),
        SaveFailure::PartiallyApplied {
            statement_index,
            source,
        } => crate::tr!(
            "Save failed at statement {n}: {error}. This database cannot roll a save back, so the statements before it were applied. Refresh the table before saving again."
        )
        .replace("{n}", &statement_index.saturating_add(1).to_string())
        .replace("{error}", driver_failure_message(source).trim_end_matches('.')),
        SaveFailure::RollbackFailed {
            statement_index,
            source,
            rollback_error,
        } => crate::tr!(
            "Save failed at statement {n}: {error}. Rollback also failed: {rollback_error}. Database changes may have been applied."
        )
        .replace("{n}", &statement_index.saturating_add(1).to_string())
        .replace("{error}", driver_failure_message(source).trim_end_matches('.'))
        .replace(
            "{rollback_error}",
            driver_failure_message(rollback_error).trim_end_matches('.'),
        ),
    }
}
