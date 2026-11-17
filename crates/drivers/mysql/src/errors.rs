//! Mapping of sqlx failures onto DriverError.

use super::*;

pub(crate) fn map_sqlx_error(err: sqlx::Error) -> DriverError {
    use sqlx::Error::*;
    match err {
        Database(e) => DriverError::Query {
            message: e.message().to_string(),
            sqlstate: e.code().map(|c| c.to_string()),
            position: None,
        },
        Io(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => DriverError::ConnectionRefused,
        Io(e) if bookie_core::looks_like_tls_failure(&bookie_core::error_chain_text(&e)) => {
            DriverError::Tls(bookie_core::error_chain_text(&e))
        }
        Io(_) => DriverError::Disconnected,
        Tls(e) => DriverError::Tls(e.to_string()),
        Configuration(e) if e.to_string().starts_with(PLAINTEXT_PASSWORD_REFUSAL) => {
            DriverError::PolicyDenied(e.to_string())
        }
        Protocol(message) if message.starts_with("mysql_cleartext_plugin disabled") => DriverError::PolicyDenied(
            "Cleartext MySQL password authentication is disabled. Configure the server account to use a supported authentication plugin, such as caching_sha2_password, and require TLS.".into(),
        ),
        PoolClosed | PoolTimedOut => DriverError::Disconnected,
        other => DriverError::Internal(format!("{other}")),
    }
}

pub(crate) fn map_sqlx_connect_error(err: sqlx::Error) -> DriverError {
    if is_pool_startup_timeout(&err) {
        // SQLx retries refused pool connections until the acquire deadline,
        // then erases the underlying I/O error into PoolTimedOut. During
        // initial setup there is no established connection to disconnect.
        DriverError::ConnectionRefused
    } else {
        map_sqlx_error(err)
    }
}

pub(crate) fn is_pool_startup_timeout(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::PoolTimedOut)
}
