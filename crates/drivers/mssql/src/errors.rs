//! Mapping of IO, TDS and server failures onto DriverError.

use super::*;

pub(crate) fn map_io_error(e: std::io::Error) -> DriverError {
    if e.kind() == std::io::ErrorKind::ConnectionRefused {
        DriverError::ConnectionRefused
    } else {
        DriverError::Internal(e.to_string())
    }
}

pub(crate) fn map_tiberius_error(err: tiberius::error::Error) -> DriverError {
    use tiberius::error::Error as E;
    match err {
        E::Io { .. } => DriverError::Disconnected,
        E::Tls(msg) => DriverError::Tls(msg),
        E::Protocol(message) if message.contains("does not allow the requested encryption level") => {
            DriverError::Tls(ENCRYPTION_DECLINED.into())
        }
        E::Server(token) => map_server_error(token.code(), token.message(), token.state()),
        #[cfg(feature = "kerberos")]
        E::Gssapi(detail) => DriverError::IntegratedAuth(detail),
        E::Routing { host, port } => DriverError::Internal(format!("server requested routing to {host}:{port}")),
        other => DriverError::Internal(other.to_string()),
    }
}

pub(crate) fn map_server_error(code: u32, message: &str, state: u8) -> DriverError {
    match code {
        // Surface login failure as an auth error so the UI shows the right
        // remediation instead of a raw SQL error.
        18456 => DriverError::AuthFailed,
        // SQL Server emits 596 after the session is killed during result
        // streaming. The socket may remain open, but the session cannot
        // deliver the rest of the result.
        596 => DriverError::Disconnected,
        _ => DriverError::Query {
            message: message.to_string(),
            sqlstate: Some(state.to_string()),
            position: None,
        },
    }
}
