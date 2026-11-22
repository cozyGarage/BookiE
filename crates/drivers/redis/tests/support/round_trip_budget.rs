use std::time::Duration;

use bookie_core::{DatabaseDriver, Value};
use futures::StreamExt;

use super::{RedisDriver, opts, start_redis};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_simple_get_uses_one_redis_protocol_command() {
    let (_container, host, port) = start_redis().await;
    let connection = RedisDriver
        .connect(opts(&host, port, "0"))
        .await
        .expect("connect BookiE");
    let key = "bookie_round_trip_budget_redis";
    connection
        .query(&format!("SET {key} fixture"))
        .await
        .expect("seed fixture");
    connection.query("PING").await.expect("warm BookiE connection");

    let monitor_client = redis::Client::open(format!("redis://{host}:{port}/0")).expect("monitor client");
    let monitor = monitor_client.get_async_monitor().await.expect("start Redis MONITOR");
    let mut messages = monitor.into_on_message::<String>();
    let result = connection.query(&format!("GET {key}")).await.expect("run measured GET");
    assert_eq!(result.rows, vec![vec![Value::Text("fixture".into())]]);

    let mut commands = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = messages.next().await {
            commands.push(message);
            if commands.last().is_some_and(|message| message.contains(key)) {
                break;
            }
        }
    })
    .await
    .expect("Redis MONITOR reports the measured GET");
    assert_eq!(
        commands.len(),
        1,
        "simple GET expected one protocol command, observed:\n{}",
        commands.join("\n")
    );
    assert!(commands[0].contains("GET") && commands[0].contains(key));
}
