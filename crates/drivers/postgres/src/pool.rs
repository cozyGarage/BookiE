use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

pub(crate) fn options(acquire_timeout: Duration) -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(acquire_timeout)
        .after_release(|connection, _| {
            Box::pin(async move { Ok(sqlx::query("DISCARD ALL").execute(connection).await.is_ok()) })
        })
}
