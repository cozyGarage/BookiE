use std::time::Duration;

use sqlx::{Connection, postgres::PgPoolOptions};

pub(crate) fn options(acquire_timeout: Duration) -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(acquire_timeout)
        .after_release(|connection, _| {
            Box::pin(async move {
                if connection.clear_cached_statements().await.is_err() {
                    return Ok(false);
                }
                Ok(sqlx::raw_sql("DISCARD ALL").execute(connection).await.is_ok())
            })
        })
}
