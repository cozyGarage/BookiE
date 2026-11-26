use std::time::Duration;

use sqlx::Connection;
use sqlx::postgres::PgPoolOptions;

// shortcut: a connection that was idle for under this long is trusted without a ping, so a server that
// went away in that window fails one statement; widen the check if that ever shows up in practice.
const TRUSTED_IDLE: Duration = Duration::from_secs(2);

pub(crate) fn options(acquire_timeout: Duration) -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(acquire_timeout)
        .test_before_acquire(false)
        .before_acquire(|connection, metadata| {
            Box::pin(async move {
                if metadata.idle_for < TRUSTED_IDLE {
                    return Ok(true);
                }
                Ok(connection.ping().await.is_ok())
            })
        })
        .after_release(|connection, _| {
            Box::pin(async move { Ok(sqlx::raw_sql("DISCARD ALL").execute(connection).await.is_ok()) })
        })
}
