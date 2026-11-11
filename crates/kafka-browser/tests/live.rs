#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use bookie_kafka_browser::{list_topics, tail};
use rskafka::client::ClientBuilder;
use rskafka::client::partition::{Compression, UnknownTopicHandling};
use rskafka::record::Record;
use testcontainers_modules::kafka::apache;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
#[ignore = "requires docker"]
async fn topics_report_their_offsets_and_a_tail_returns_the_newest_records() {
    let node = apache::Kafka::default().start().await.unwrap();
    let port = node.get_host_port_ipv4(apache::KAFKA_PORT).await.unwrap();
    let brokers = vec![format!("127.0.0.1:{port}")];

    let admin = ClientBuilder::new(brokers.clone()).build().await.unwrap();
    admin
        .controller_client()
        .unwrap()
        .create_topic("orders", 1, 1, 5_000)
        .await
        .unwrap();
    let producer = admin
        .partition_client("orders", 0, UnknownTopicHandling::Retry)
        .await
        .unwrap();
    let records: Vec<Record> = (0..5)
        .map(|n| Record {
            key: Some(format!("k{n}").into_bytes()),
            value: Some(format!("order {n}").into_bytes()),
            headers: Default::default(),
            timestamp: chrono::Utc::now(),
        })
        .collect();
    producer.produce(records, Compression::NoCompression).await.unwrap();

    let topics = list_topics(&brokers).await.unwrap();
    let orders = topics.iter().find(|t| t.name == "orders").expect("orders listed");
    assert_eq!(orders.partitions.len(), 1);
    assert_eq!((orders.partitions[0].earliest, orders.partitions[0].latest), (0, 5));
    assert_eq!(orders.partitions[0].retained(), 5);

    let newest = tail(&brokers, "orders", 0, 3).await.unwrap();
    let shown: Vec<(i64, &str, Option<&str>)> = newest
        .iter()
        .map(|r| (r.offset, r.value.as_str(), r.key.as_deref()))
        .collect();
    assert_eq!(
        shown,
        [
            (2, "order 2", Some("k2")),
            (3, "order 3", Some("k3")),
            (4, "order 4", Some("k4"))
        ]
    );
    assert!(tail(&brokers, "orders", 0, 50).await.unwrap().len() == 5);
}

#[tokio::test]
async fn an_unreachable_broker_is_reported_not_hung() {
    let error = list_topics(&["127.0.0.1:1".to_string()]).await.unwrap_err();
    assert!(
        matches!(error, bookie_kafka_browser::KafkaError::Unreachable(_)),
        "{error}"
    );
}
