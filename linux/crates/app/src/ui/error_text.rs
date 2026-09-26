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
        BuildSqlError::StaleColumns => crate::tr!(
            "This table's columns changed after you started editing (for example, in a Structure tab). \
             Reload the page and reapply your changes."
        ),
    }
}

pub fn driver_message(error: &DriverError) -> String {
    match error {
        DriverError::ConnectionRefused => crate::tr!("Could not reach the database. Is it running?"),
        DriverError::AuthFailed => crate::tr!("Username or password is wrong."),
        DriverError::Tls(detail) => crate::tr!("TLS handshake failed: {detail}").replace("{detail}", detail),
        DriverError::Query {
            message,
            sqlstate: Some(s),
        } => crate::tr!("Query failed (SQLSTATE {sqlstate}): {message}")
            .replace("{sqlstate}", s)
            .replace("{message}", message),
        DriverError::Query { message, .. } => crate::tr!("Query failed: {message}").replace("{message}", message),
        DriverError::Disconnected => crate::tr!("The connection was closed. Try reconnecting."),
        DriverError::ReadOnly => {
            crate::tr!("This connection is read-only. Reopen it without read-only mode to make changes.")
        }
        DriverError::PolicyDenied(detail) => crate::tr!("Blocked by policy: {detail}").replace("{detail}", detail),
        DriverError::Unsupported(detail) => {
            crate::tr!("This driver does not support: {detail}").replace("{detail}", detail)
        }
        DriverError::Internal(detail) => crate::tr!("Internal driver error: {detail}").replace("{detail}", detail),
        DriverError::Cancelled => crate::tr!("The operation was cancelled."),
        DriverError::TimedOut => crate::tr!("The operation timed out."),
        DriverError::OperationOutcomeUnknown { source } => crate::tr!(
            "The operation was interrupted, but the database outcome could not be confirmed: {detail}"
        )
        .replace("{detail}", &driver_message(source)),
        DriverError::IntegratedAuth(detail) => crate::tr!(
            "Kerberos login failed: {detail}. Check that klist shows a valid ticket, run kinit if it does not, and make sure the server's SPN matches the host you typed."
        )
        .replace("{detail}", detail),
        DriverError::Transaction {
            statement_index,
            source,
        } => {
            crate::tr!("Save failed at statement {n}: {error}. The transaction was rolled back; no rows were changed.")
                .replace("{n}", &(statement_index + 1).to_string())
                .replace("{error}", &driver_message(source))
        }
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
mod tests {

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
        });
        assert!(with_state.contains("23505"));
        let without = driver_message(&DriverError::Query {
            message: "syntax error".into(),
            sqlstate: None,
        });
        assert!(!without.contains("SQLSTATE"));
        assert!(without.contains("syntax error"));
    }

    #[test]
    fn driver_message_for_simple_variants() {
        assert!(driver_message(&DriverError::ConnectionRefused).contains("Could not reach"));
        assert!(driver_message(&DriverError::AuthFailed).contains("wrong"));
        assert!(driver_message(&DriverError::Disconnected).contains("Try reconnecting"));
    }

    #[test]
    fn integrated_auth_names_the_remedy_and_keeps_the_detail() {
        let message = driver_message(&DriverError::IntegratedAuth("No Kerberos credentials available".into()));
        assert!(message.contains("No Kerberos credentials available"));
        assert!(message.contains("kinit"));
        assert!(message.contains("SPN"));
    }
}
