#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::time::Duration;

use bookie_core::{ConnectOptions, TlsConfig};
use drivers_redis::sample_pubsub;
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_pattern_sample_collects_published_messages_up_to_the_limit() {
    let container = Redis::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let options = ConnectOptions {
        host: host.clone(),
        port,
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let sampling =
        tokio::spawn(async move { sample_pubsub(options, &["news.*".to_string()], 3, Duration::from_secs(20)).await });
    tokio::time::sleep(Duration::from_secs(2)).await;
    let publisher = redis::Client::open(format!("redis://{host}:{port}")).unwrap();
    let mut connection = publisher.get_multiplexed_async_connection().await.unwrap();
    for n in 0..5 {
        let _: i64 = redis::cmd("PUBLISH")
            .arg(format!("news.{n}"))
            .arg(format!("item {n}"))
            .query_async(&mut connection)
            .await
            .unwrap();
        let _: i64 = redis::cmd("PUBLISH")
            .arg("other")
            .arg("ignored")
            .query_async(&mut connection)
            .await
            .unwrap();
    }
    let sample = sampling.await.unwrap().unwrap();
    assert!(sample.reached_limit);
    let channels: Vec<&str> = sample.messages.iter().map(|m| m.channel.as_str()).collect();
    assert_eq!(channels, ["news.0", "news.1", "news.2"], "{sample:?}");
    assert_eq!(sample.messages[1].payload, "item 1");
    assert_eq!(sample.messages[0].pattern.as_deref(), Some("news.*"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_quiet_channel_ends_at_the_window_with_no_messages() {
    let container = Redis::default().start().await.unwrap();
    let options = ConnectOptions {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(6379).await.unwrap(),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let sample = sample_pubsub(options, &["quiet".to_string()], 5, Duration::from_secs(2))
        .await
        .unwrap();
    assert!(sample.messages.is_empty());
    assert!(!sample.reached_limit);
}
