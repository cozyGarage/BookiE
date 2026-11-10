use bookie_core::{CommitResolution, CommitToken, DriverError};
use sqlx::PgPool;

use crate::map_sqlx_error;

pub(crate) async fn resolve(pool: &PgPool, token: &CommitToken) -> Result<CommitResolution, DriverError> {
    let CommitToken::PostgresXid { xid, legacy_txid } = token else {
        return Ok(CommitResolution::Unresolvable);
    };
    let sql = if *legacy_txid {
        "SELECT pg_catalog.txid_status($1::text::int8)::text"
    } else {
        "SELECT pg_catalog.pg_xact_status($1::text::xid8)::text"
    };
    let status: Option<String> = sqlx::query_scalar(sql)
        .bind(xid.to_string())
        .fetch_one(pool)
        .await
        .map_err(map_sqlx_error)?;
    Ok(resolution(status.as_deref()))
}

fn resolution(status: Option<&str>) -> CommitResolution {
    match status {
        Some("committed") => CommitResolution::Applied,
        Some("aborted") => CommitResolution::NotApplied,
        Some("in progress") => CommitResolution::StillRunning,
        _ => CommitResolution::Unresolvable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_servers_answers_map_to_resolutions() {
        assert_eq!(resolution(Some("committed")), CommitResolution::Applied);
        assert_eq!(resolution(Some("aborted")), CommitResolution::NotApplied);
        assert_eq!(resolution(Some("in progress")), CommitResolution::StillRunning);
        assert_eq!(resolution(None), CommitResolution::Unresolvable);
    }
}
