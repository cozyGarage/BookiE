#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use drivers_redis::RedisDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver, TlsConfig, Value};
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::{ContainerAsync, GenericImage};
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

async fn start_redis() -> (ContainerAsync<Redis>, String, u16) {
    let container = Redis::default().start().await.expect("start redis container");
    let host = container.get_host().await.expect("host").to_string();
    let port = container.get_host_port_ipv4(6379).await.expect("port");
    (container, host, port)
}

async fn start_redis_resp3() -> (ContainerAsync<GenericImage>, String, u16) {
    let container = GenericImage::new("redis", "7.4-alpine")
        .with_exposed_port(6379.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .await
        .expect("start Redis 7.4 fixture for RESP3");
    let host = container.get_host().await.expect("host").to_string();
    let port = container.get_host_port_ipv4(6379).await.expect("port");
    (container, host, port)
}

fn opts(host: &str, port: u16, database: &str) -> ConnectOptions {
    ConnectOptions {
        host: host.to_string(),
        port,
        database: database.to_string(),
        username: String::new(),
        password: secrecy::SecretString::new(String::new().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    }
}

/// fetch_rows() browses a table named after a database other than the
/// connection's own -- it must restore the connection's own database
/// before returning, or the next call on this shared connection lands
/// wherever the browse last left it.
#[tokio::test]
#[ignore = "requires docker"]
async fn browsing_another_database_does_not_leak_into_later_queries() {
    let (_container, host, port) = start_redis().await;
    let conn = RedisDriver.connect(opts(&host, port, "0")).await.expect("connect");

    conn.query("SET home_key 1").await.expect("seed db0");

    conn.fetch_rows(None, "db2", 0, 10).await.expect("browse db2");

    let result = conn
        .query("GET home_key")
        .await
        .expect("read back on the connection's own db");
    assert_eq!(result.rows, vec![vec![Value::Text("1".into())]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_preserves_integer_and_text_command_arguments() {
    let (_container, host, port) = start_redis().await;
    let connection = RedisDriver.connect(opts(&host, port, "0")).await.unwrap();
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../../testdata/value-contract.json")).unwrap();
    for integer in corpus["integers"].as_array().unwrap() {
        let integer = integer.as_str().unwrap();
        connection.query(&format!("SET boundary {integer}")).await.unwrap();
        assert_eq!(
            connection.query("INCRBY boundary 0").await.unwrap().rows,
            vec![vec![Value::Int(integer.parse().unwrap())]]
        );
    }
    for text in corpus["texts"].as_array().unwrap() {
        let text = text.as_str().unwrap();
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
        connection.query(&format!("SET boundary \"{escaped}\"")).await.unwrap();
        assert_eq!(
            connection.query("GET boundary").await.unwrap().rows,
            vec![vec![Value::Text(text.into())]]
        );
    }
    assert_eq!(
        connection.query("GET missing_value").await.unwrap().rows,
        vec![vec![Value::Null]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_resp3_hash_map_preserves_binary_fields_and_values() {
    struct BinaryBulk(Vec<u8>);
    impl redis::ToRedisArgs for BinaryBulk {
        fn write_redis_args<W>(&self, out: &mut W)
        where
            W: ?Sized + redis::RedisWrite,
        {
            out.write_arg(&self.0);
        }
    }

    let (_container, host, port) = start_redis_resp3().await;
    let native = redis::Client::open(format!("redis://{host}:{port}/0")).unwrap();
    let mut native_connection = native.get_multiplexed_async_connection().await.unwrap();
    redis::cmd("HSET")
        .arg("value_contract_hash")
        .arg("name")
        .arg("Ada")
        .arg("opaque")
        .arg(BinaryBulk(vec![0xFF, 0x00, 0x41]))
        .query_async::<i64>(&mut native_connection)
        .await
        .expect("seed text and binary hash values");
    redis::cmd("XADD")
        .arg("value_contract_stream")
        .arg("*")
        .arg("sensor")
        .arg("temperature")
        .arg("opaque")
        .arg(BinaryBulk(vec![0xFF, 0x00, 0x41]))
        .query_async::<String>(&mut native_connection)
        .await
        .expect("seed nested stream entry");

    let connection = RedisDriver.connect(opts(&host, port, "0")).await.unwrap();
    let hello = connection
        .query("HELLO 3")
        .await
        .expect("switch this connection to RESP3");
    assert!(
        hello
            .rows
            .iter()
            .any(|row| row == &[Value::Text("proto".into()), Value::Int(3)]),
        "HELLO must return a structured RESP3 map: {:?}",
        hello.rows
    );

    let result = connection
        .query("HGETALL value_contract_hash")
        .await
        .expect("read RESP3 hash map");
    assert_eq!(
        result
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["key", "value"]
    );
    assert!(
        result
            .rows
            .iter()
            .any(|row| { row == &[Value::Text("name".into()), Value::Text("Ada".into())] })
    );
    assert!(
        result
            .rows
            .iter()
            .any(|row| { row == &[Value::Text("opaque".into()), Value::Bytes(vec![0xFF, 0x00, 0x41])] }),
        "RESP3 binary hash value changed: {:?}",
        result.rows
    );

    let stream = connection
        .query("XREAD STREAMS value_contract_stream 0")
        .await
        .expect("read RESP3 nested stream map");
    assert_eq!(stream.rows.len(), 1, "one stream should produce one outer row");
    assert_eq!(stream.rows[0][0], Value::Text("value_contract_stream".into()));
    let Value::Json(nested) = &stream.rows[0][1] else {
        panic!("nested stream entries must not be flattened: {:?}", stream.rows[0][1]);
    };
    assert!(nested.to_string().contains("sensor"), "{nested}");
    assert!(nested.to_string().contains(r#""$redisBytes":"ff0041""#), "{nested}");
    assert!(!nested.to_string().contains("BulkString"), "{nested}");
}
