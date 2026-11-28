#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{Connection, DriverError, Value};
use bookie_data_integrity::governed;
use bookie_data_integrity::redis;

#[tokio::test]
#[ignore = "requires docker"]
async fn redis_read_only_commands_pass_the_guard_without_permitting_writes() {
    let server = redis::start("redis-7.4").await;
    let writer = governed::open_confirming("redis", server.options.clone()).await;
    writer.guard.execute("SET sample original").await.unwrap();
    writer.guard.execute("HSET hash field value").await.unwrap();
    writer.guard.execute("LPUSH list item").await.unwrap();
    writer.guard.execute("SADD members item").await.unwrap();
    writer.guard.execute("ZADD ranked 1 item").await.unwrap();
    writer.guard.execute("XADD stream * field value").await.unwrap();
    writer.close().await;
    let reader = governed::open("redis", server.options.clone(), true).await;
    let result = reader.guard.query("GET sample").await.unwrap();
    assert_eq!(result.rows[0][0], Value::Text("original".into()));
    for command in [
        "MGET sample missing",
        "TYPE sample",
        "TTL sample",
        "HGET hash field",
        "HGETALL hash",
        "LRANGE list 0 -1",
        "SMEMBERS members",
        "ZRANGE ranked 0 -1",
        "XRANGE stream - +",
        "SCAN 0",
        "PING",
    ] {
        reader
            .guard
            .query(command)
            .await
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    for command in [
        "SET sample changed",
        "DEL sample",
        "EVAL 'return 1' 0",
        "UNKNOWN sample",
        "SELECT 1",
        "GET \"unterminated",
    ] {
        assert!(
            matches!(reader.guard.query(command).await, Err(DriverError::PolicyDenied(_))),
            "{command}"
        );
    }
    assert_eq!(
        reader.guard.query("GET sample").await.unwrap().rows[0][0],
        Value::Text("original".into())
    );
    reader.close().await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn redis_7_4_reads_match_the_server_replies() {
    let corpus = redis::load();
    let server = redis::start("redis-7.4").await;
    let results = redis::run(&server, &corpus).await;
    redis::judge(&corpus, &results).assert_clean();
}
