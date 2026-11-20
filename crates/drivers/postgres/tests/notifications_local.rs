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

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn a_follow_streams_messages_until_it_is_cancelled() {
    use drivers_postgres::follow_notifications;
    let cancel = tokio_util::sync::CancellationToken::new();
    let (seen_tx, mut seen) = tokio::sync::mpsc::unbounded_channel();
    let stop = cancel.clone();
    let channels = vec!["bookie_follow_jobs".to_string()];
    let follower = tokio::spawn(async move {
        follow_notifications(opts_from_env(), &channels, &stop, move |message| {
            let _ = seen_tx.send(message);
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(800)).await;
    let sender = PgDriver.connect(opts_from_env()).await.unwrap();
    for payload in ["one", "two", "three"] {
        sender
            .execute(&format!("SELECT pg_notify('bookie_follow_jobs', '{payload}')"))
            .await
            .unwrap();
    }
    let mut got = Vec::new();
    for _ in 0..3 {
        let message = tokio::time::timeout(Duration::from_secs(5), seen.recv())
            .await
            .unwrap()
            .unwrap();
        got.push(message.payload);
    }
    assert_eq!(got, ["one", "two", "three"]);
    cancel.cancel();
    let received = tokio::time::timeout(Duration::from_secs(5), follower)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(received, 3, "a cancelled follow ends cleanly with its count");
}

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn a_follow_ends_with_an_error_when_the_server_drops_it() {
    use drivers_postgres::follow_notifications;
    let cancel = tokio_util::sync::CancellationToken::new();
    let channels = vec!["bookie_follow_drop".to_string()];
    let stop = cancel.clone();
    let follower = tokio::spawn(async move {
        let mut options = opts_from_env();
        options.application_name = Some("claude_follow_victim".into());
        follow_notifications(options, &channels, &stop, |_| {}).await
    });
    tokio::time::sleep(Duration::from_millis(800)).await;
    let admin = PgDriver.connect(opts_from_env()).await.unwrap();
    admin
        .execute(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name = 'claude_follow_victim'",
        )
        .await
        .unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(10), follower)
        .await
        .unwrap()
        .unwrap();
    assert!(
        ended.is_err(),
        "a dropped listener must end the follow with an error, not hang"
    );
}
