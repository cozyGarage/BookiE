//! Mapping of sqlx and server failures onto `DriverError`.

use super::*;

pub(crate) fn is_certificate_failure(err: &std::io::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(current) = source {
        let text = current.to_string().to_ascii_lowercase();
        if text.contains("certificate") || text.contains("tls handshake") {
            return true;
        }
        source = current.source();
    }
    false
}

pub(crate) fn map_sqlx_error(err: sqlx::Error) -> DriverError {
    if is_enum_label_metadata_null(&err) {
        return DriverError::Unsupported(
            "PostgreSQL enum result metadata could not be resolved; the result was not read".into(),
        );
    }
    if matches!(&err, sqlx::Error::Protocol(message) if message.starts_with("unable to resolve type OIDs:")) {
        return DriverError::Unsupported("PostgreSQL type hierarchy exceeds the driver's resolvable depth".into());
    }
    use sqlx::Error::*;
    match err {
        Database(e) => {
            let sqlstate = e.code().map(|c| c.to_string());
            if is_server_disconnect_sqlstate(sqlstate.as_deref()) {
                DriverError::Disconnected
            } else {
                DriverError::Query {
                    message: with_pooler_hint(e.message(), sqlstate.as_deref()),
                    sqlstate,
                    position: statement_position(e.as_ref()),
                }
            }
        }
        Io(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => DriverError::ConnectionRefused,
        Io(e) if is_certificate_failure(&e) => DriverError::Tls(e.to_string()),
        Io(_) => DriverError::Disconnected,
        Tls(e) => DriverError::Tls(e.to_string()),
        PoolClosed | PoolTimedOut => DriverError::Disconnected,
        other => DriverError::Internal(format!("{other}")),
    }
}

pub(crate) fn with_pooler_hint(message: &str, sqlstate: Option<&str>) -> String {
    let prepared_statement_conflict = matches!(sqlstate, Some("42P05" | "26000")) && message.contains("\"sqlx_s_");
    let unsupported_binary_bind =
        sqlstate == Some("22P03") && message.starts_with("incorrect binary data format in bind parameter ");
    if prepared_statement_conflict || unsupported_binary_bind {
        return format!(
            "{message} (the server connection may be behind a pooler in transaction mode: enable its prepared statement support, for example PgBouncer max_prepared_statements, or use session pooling)"
        );
    }
    message.to_string()
}

pub(crate) fn is_enum_label_metadata_null(err: &sqlx::Error) -> bool {
    let sqlx::Error::ColumnDecode { index, source } = err else {
        return false;
    };
    index == "\"enum_labels\"" && source.downcast_ref::<sqlx::error::UnexpectedNullError>().is_some()
}

pub(crate) fn map_sqlx_connect_error(err: sqlx::Error) -> DriverError {
    if is_pool_startup_timeout(&err) {
        // SQLx retries refused pool connections until the acquire deadline,
        // then erases the underlying I/O error into PoolTimedOut. During
        // initial setup there is no established connection to disconnect.
        DriverError::ConnectionRefused
    } else if matches!(&err, sqlx::Error::Database(e) if e.code().as_deref() == Some("28P01")) {
        DriverError::AuthFailed
    } else {
        map_sqlx_error(err)
    }
}

pub(crate) fn is_pool_startup_timeout(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::PoolTimedOut)
}

pub(crate) fn statement_position(error: &dyn sqlx::error::DatabaseError) -> Option<usize> {
    match error
        .try_downcast_ref::<sqlx::postgres::PgDatabaseError>()?
        .position()?
    {
        sqlx::postgres::PgErrorPosition::Original(position) => Some(position),
        sqlx::postgres::PgErrorPosition::Internal { .. } => None,
    }
}

pub(crate) fn is_server_disconnect_sqlstate(sqlstate: Option<&str>) -> bool {
    matches!(sqlstate, Some("25P03" | "57P01" | "57P02" | "57P03" | "57P04"))
}
