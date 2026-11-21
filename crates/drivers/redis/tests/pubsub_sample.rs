#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::time::Duration;

use bookie_core::{ConnectOptions, TlsConfig};
use drivers_redis::sample_pubsub;
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const REDIS_TEST_TAG: &str = "7.4.12-alpine3.22";

#[tokio::test]
#[ignore = "requires docker"]
async fn a_pattern_sample_collects_published_messages_up_to_the_limit() {
    let container = Redis::default().with_tag(crate::REDIS_TEST_TAG).start().await.unwrap();
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
    let container = Redis::default().with_tag(crate::REDIS_TEST_TAG).start().await.unwrap();
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

#[tokio::test]
#[ignore = "requires docker"]
async fn a_follow_streams_until_cancelled_and_leaves_no_subscriber() {
    let container = Redis::default().with_tag(crate::REDIS_TEST_TAG).start().await.unwrap();
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let options = ConnectOptions {
        host: host.clone(),
        port,
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (sink, stop) = (seen.clone(), cancel.clone());
    let following = tokio::spawn(async move {
        drivers_redis::follow_pubsub(options, &["jobs.*".to_string()], &stop, move |m| {
            sink.lock().unwrap().push(m.payload)
        })
        .await
    });
    let publisher = redis::Client::open(format!("redis://{host}:{port}")).unwrap();
    let mut connection = publisher.get_multiplexed_async_connection().await.unwrap();
    let count = |connection: &mut redis::aio::MultiplexedConnection| {
        let mut connection = connection.clone();
        async move {
            redis::cmd("PUBSUB")
                .arg("NUMPAT")
                .query_async::<i64>(&mut connection)
                .await
                .unwrap()
        }
    };
    for _ in 0..40 {
        if count(&mut connection).await == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    for n in 0..3 {
        let _: i64 = redis::cmd("PUBLISH")
            .arg("jobs.a")
            .arg(format!("m{n}"))
            .query_async(&mut connection)
            .await
            .unwrap();
    }
    for _ in 0..40 {
        if seen.lock().unwrap().len() == 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    cancel.cancel();
    assert_eq!(following.await.unwrap().unwrap(), 3);
    assert_eq!(*seen.lock().unwrap(), ["m0", "m1", "m2"]);
    for _ in 0..40 {
        if count(&mut connection).await == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the pattern subscription outlived the follow");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_follow_ends_as_disconnected_when_the_server_goes_away() {
    let container = Redis::default().with_tag(crate::REDIS_TEST_TAG).start().await.unwrap();
    let options = ConnectOptions {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(6379).await.unwrap(),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let following =
        tokio::spawn(async move { drivers_redis::follow_pubsub(options, &["x".to_string()], &cancel, |_| {}).await });
    tokio::time::sleep(Duration::from_secs(2)).await;
    container.stop().await.unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(20), following)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(ended, Err(bookie_core::DriverError::Disconnected)),
        "{ended:?}"
    );
}
