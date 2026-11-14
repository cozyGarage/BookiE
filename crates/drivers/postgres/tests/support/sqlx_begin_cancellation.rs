use crate::start_pg;
use secrecy::ExposeSecret;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::time::Duration;

#[tokio::test]
#[ignore = "requires docker"]
async fn cancelled_begin_rolls_back_before_pool_reuses_connection() {
    let (_container, options) = start_pg().await;
    let pg_options = PgConnectOptions::new()
        .host(&options.host)
        .port(options.port)
        .database(&options.database)
        .username(&options.username)
        .password(options.password.expose_secret())
        .application_name("bookie_sqlx_begin_cancel");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pg_options.clone())
        .await
        .expect("connect one-connection pool");
    let observer = PgPoolOptions::new()
        .connect_with(pg_options)
        .await
        .expect("connect observer pool");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&pool)
        .await
        .expect("read pooled backend pid");
    let begin_pool = pool.clone();
    let begin = tokio::spawn(async move {
        begin_pool
            .begin_with("BEGIN; SELECT pg_sleep(2); /* bookie_sqlx_begin_cancel */")
            .await
    });

    wait_for_tagged_begin(&observer, pid).await;
    begin.abort();
    assert!(begin.await.expect_err("cancelled task").is_cancelled());
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&pool)
            .await
            .expect("reuse pooled connection")
    })
    .await
    .expect("pool connection returns after the server finishes the query");
    assert_eq!(result, 1);

    let state: Option<String> = sqlx::query_scalar("SELECT state FROM pg_stat_activity WHERE pid = $1")
        .bind(pid)
        .fetch_optional(&observer)
        .await
        .expect("read transaction state");
    assert_eq!(state.as_deref(), Some("idle"));
}

async fn wait_for_tagged_begin(observer: &sqlx::Pool<sqlx::Postgres>, pid: i32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let active: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid = $1 AND state = 'active' AND query LIKE '%bookie_sqlx_begin_cancel%')",
            )
            .bind(pid)
            .fetch_one(observer)
            .await
            .expect("inspect begin query");
            if active {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("begin query reaches PostgreSQL");
}
