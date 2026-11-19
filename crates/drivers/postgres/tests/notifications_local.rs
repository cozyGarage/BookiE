#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Notification sampler against an already-running Postgres (no Docker required).
//!
//! Ignored by default. Env: SMOKE_PG_HOST, SMOKE_PG_PORT, SMOKE_PG_USER, SMOKE_PG_PASS, SMOKE_PG_DB,
//! the same as `smoke_local`. It only sends NOTIFY on its own channel names.

use std::time::Duration;

use bookie_core::{ConnectOptions, DatabaseDriver};
use drivers_postgres::{PgDriver, sample_notifications};

fn opts_from_env() -> ConnectOptions {
    let port = std::env::var("SMOKE_PG_PORT").unwrap_or_else(|_| "54329".into());
    ConnectOptions {
        host: std::env::var("SMOKE_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: port
            .parse()
            .unwrap_or_else(|error| panic!("SMOKE_PG_PORT={port} is not a port number: {error}")),
        database: std::env::var("SMOKE_PG_DB").unwrap_or_else(|_| "bookie".into()),
        username: std::env::var("SMOKE_PG_USER").unwrap_or_else(|_| "bookie".into()),
        password: secrecy::SecretString::new(
            std::env::var("SMOKE_PG_PASS")
                .unwrap_or_else(|_| "bookie".into())
                .into(),
        ),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn notifications_sent_while_listening_are_collected_up_to_the_limit() {
    let channels = vec!["bookie_sample_jobs".to_string(), "bookie_sample_other".to_string()];
    let options = opts_from_env();
    let sampler =
        tokio::spawn(async move { sample_notifications(options, &channels, 3, Duration::from_secs(8)).await });
    tokio::time::sleep(Duration::from_millis(800)).await;
    let sender = PgDriver.connect(opts_from_env()).await.unwrap();
    for (channel, payload) in [
        ("bookie_sample_jobs", "one"),
        ("bookie_sample_other", "two"),
        ("bookie_sample_jobs", ""),
    ] {
        sender
            .execute(&format!("SELECT pg_notify('{channel}', '{payload}')"))
            .await
            .unwrap();
    }
    sender
        .execute("SELECT pg_notify('bookie_sample_unwatched', 'ignored')")
        .await
        .unwrap();
    let sample = sampler.await.unwrap().unwrap();
    assert!(sample.reached_limit);
    let seen: Vec<(&str, &str)> = sample
        .messages
        .iter()
        .map(|message| (message.channel.as_str(), message.payload.as_str()))
        .collect();
    assert_eq!(
        seen,
        [
            ("bookie_sample_jobs", "one"),
            ("bookie_sample_other", "two"),
            ("bookie_sample_jobs", "")
        ]
    );
}

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn a_quiet_channel_ends_at_the_window_without_reaching_the_limit() {
    let started = std::time::Instant::now();
    let sample = sample_notifications(
        opts_from_env(),
        &["bookie_sample_quiet".to_string()],
        5,
        Duration::from_millis(1200),
    )
    .await
    .unwrap();
    assert!(sample.messages.is_empty());
    assert!(!sample.reached_limit);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the window bounds the wait"
    );
}

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn a_wrong_password_is_reported_not_retried() {
    let mut options = opts_from_env();
    options.password = secrecy::SecretString::new("definitely-not-the-password".to_string().into());
    let result = sample_notifications(options, &["bookie_sample_jobs".to_string()], 1, Duration::from_secs(1)).await;
    assert!(result.is_err());
}
