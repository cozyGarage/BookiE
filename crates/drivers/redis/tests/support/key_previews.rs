#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use drivers_redis::RedisDriver;
use tablepro_core::{DatabaseDriver, Value};

use crate::{opts, start_redis};

#[tokio::test]
#[ignore = "requires docker"]
async fn every_key_type_lists_with_its_type_and_a_preview() {
    let (_container, host, port) = start_redis().await;
    let native = redis::Client::open(format!("redis://{host}:{port}/0")).unwrap();
    let mut native_connection = native.get_multiplexed_async_connection().await.unwrap();
    for command in [
        vec!["SET", "k_string", "hello"],
        vec!["HSET", "k_hash", "field", "value"],
        vec!["RPUSH", "k_list", "a", "b"],
        vec!["SADD", "k_set", "m"],
        vec!["ZADD", "k_zset", "1", "z"],
        vec!["XADD", "k_stream", "*", "f", "v"],
    ] {
        let mut cmd = redis::cmd(command[0]);
        for argument in &command[1..] {
            cmd.arg(*argument);
        }
        cmd.query_async::<redis::Value>(&mut native_connection).await.unwrap();
    }

    let connection = RedisDriver.connect(opts(&host, port, "0")).await.unwrap();
    let rows = connection.fetch_rows(None, "db0", 0, 50).await.unwrap().rows;
    let find = |key: &str| {
        rows.iter()
            .find(|row| row[0] == Value::Text(key.into()))
            .unwrap_or_else(|| panic!("{key} is listed"))
            .clone()
    };

    assert_eq!(find("k_string")[1], Value::Text("string".into()));
    assert_eq!(find("k_string")[3], Value::Text("hello".into()));
    assert_eq!(find("k_hash")[1], Value::Text("hash".into()));
    assert_eq!(find("k_hash")[3], Value::Text(r#"[("field", "value")]"#.into()));
    assert_eq!(find("k_list")[3], Value::Text(r#"["a", "b"]"#.into()));
    assert_eq!(find("k_set")[3], Value::Text(r#"["m"]"#.into()));
    assert_eq!(find("k_zset")[3], Value::Text(r#"["z"]"#.into()));
    assert_eq!(find("k_stream")[3], Value::Text("<stream>".into()));
    assert_eq!(find("k_string")[2], Value::Int(-1), "no expiry is reported as -1");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_browse_preview_of_a_large_value_stays_small() {
    let (_container, host, port) = start_redis().await;
    let native = redis::Client::open(format!("redis://{host}:{port}/0")).unwrap();
    let mut native_connection = native.get_multiplexed_async_connection().await.unwrap();
    redis::cmd("SET")
        .arg("big_string")
        .arg("x".repeat(1_048_576))
        .query_async::<()>(&mut native_connection)
        .await
        .unwrap();
    for index in 0..5_000 {
        redis::cmd("SADD")
            .arg("big_set")
            .arg(format!("member-{index}"))
            .query_async::<()>(&mut native_connection)
            .await
            .unwrap();
        redis::cmd("HSET")
            .arg("big_hash")
            .arg(format!("field-{index}"))
            .arg("value")
            .query_async::<()>(&mut native_connection)
            .await
            .unwrap();
    }

    let connection = RedisDriver.connect(opts(&host, port, "0")).await.unwrap();
    let rows = connection.fetch_rows(None, "db0", 0, 50).await.unwrap().rows;
    let preview = |key: &str| match &rows.iter().find(|row| row[0] == Value::Text(key.into())).unwrap()[3] {
        Value::Text(text) => text.clone(),
        Value::Undecodable(kind) => tablepro_core::parse_partial(kind).unwrap().1.to_owned(),
        other => panic!("{key} has no bounded preview: {other:?}"),
    };

    assert!(preview("big_string").len() < 8_192, "{}", preview("big_string").len());
    assert!(preview("big_set").matches("member-").count() <= 100);
    assert!(preview("big_hash").matches("field-").count() <= 100);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn bounded_string_previews_distinguish_complete_values_from_partial_cells() {
    let (_container, host, port) = start_redis().await;
    let native = redis::Client::open(format!("redis://{host}:{port}/0")).unwrap();
    let mut native = native.get_multiplexed_async_connection().await.unwrap();
    for (key, value) in [
        ("boundary", vec![b'x'; 4096]),
        ("long", vec![b'x'; 4097]),
        ("binary", vec![0xff; 4097]),
        ("marker", b"partial 9999 bytes\x01prefix".to_vec()),
    ] {
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .query_async::<()>(&mut native)
            .await
            .unwrap();
    }
    let connection = RedisDriver.connect(opts(&host, port, "0")).await.unwrap();
    let result = connection.fetch_rows(None, "db0", 0, 10).await.unwrap();
    let cell = |name: &str| {
        &result
            .rows
            .iter()
            .find(|row| row[0] == Value::Text(name.into()))
            .unwrap()[3]
    };
    assert_eq!(cell("boundary"), &Value::Text("x".repeat(4096)));
    assert_eq!(cell("marker"), &Value::Text("partial 9999 bytes\u{1}prefix".into()));
    let Value::Undecodable(kind) = cell("long") else {
        panic!("long values must be partial")
    };
    assert_eq!(
        tablepro_core::parse_partial(kind),
        Some((4097, "x".repeat(4096).as_str()))
    );
    assert!(
        matches!(cell("long"), Value::Undecodable(_)),
        "a prefix is not a complete text value"
    );
    assert!(
        matches!(cell("binary"), Value::Undecodable(_)),
        "a byte prefix is not a complete binary value"
    );
}
