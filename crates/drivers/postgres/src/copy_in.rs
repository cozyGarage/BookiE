use bookie_core::sql_lex::{CopyIn, copy_in_parts};
use bookie_core::{DriverError, QueryResult, QueryRows};
use sqlx::Postgres;
use sqlx::pool::PoolConnection;

use crate::errors::map_sqlx_error;

pub(crate) const MAX_COPY_DATA_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn parse(sql: &str) -> Result<Option<CopyIn<'_>>, DriverError> {
    match copy_in_parts(sql, "postgres") {
        None => Ok(None),
        Some(parts) if !parts.terminated => Err(DriverError::Unsupported(
            "COPY ... FROM STDIN data must end with a line that contains only \\.".into(),
        )),
        Some(parts) if parts.data.len() > MAX_COPY_DATA_BYTES => Err(DriverError::Unsupported(format!(
            "COPY ... FROM STDIN data is larger than {} MiB",
            MAX_COPY_DATA_BYTES / (1024 * 1024)
        ))),
        Some(parts) => Ok(Some(parts)),
    }
}

pub(crate) async fn run(connection: &mut PoolConnection<Postgres>, parts: &CopyIn<'_>) -> Result<u64, DriverError> {
    let mut copy = connection.copy_in_raw(parts.command).await.map_err(map_sqlx_error)?;
    let mut data = parts.data.as_bytes().to_vec();
    if !data.is_empty() {
        data.push(b'\n');
    }
    if let Err(error) = copy.send(data).await {
        let _ = copy.abort("the client could not send the data").await;
        return Err(map_sqlx_error(error));
    }
    copy.finish().await.map_err(map_sqlx_error)
}

pub(crate) fn empty_result() -> QueryResult {
    QueryResult {
        columns: Vec::new(),
        rows: QueryRows::with_capacity(0, 0),
        truncated: false,
    }
}
