#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../shared/tests/keyspace_conformance.rs"]
mod keyspace_conformance;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use bookie_core::{ConnectOptions, TlsConfig};
use drivers_redis::RedisDriver;
use keyspace_conformance::KeyKind;
use testcontainers::ImageExt;
use testcontainers_modules::redis::Redis as RedisImage;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const REDIS_TEST_TAG: &str = "7.4.12-alpine3.22";

static SERVER: OnceLock<(String, u16)> = OnceLock::new();
static NEXT_DATABASE: AtomicU8 = AtomicU8::new(1);

struct Redis;

struct Keyspace {
    host: String,
    port: u16,
    database: u8,
}

async fn native(scratch: &Keyspace) -> redis::aio::MultiplexedConnection {
    let url = format!("redis://{}:{}/{}", scratch.host, scratch.port, scratch.database);
    redis::Client::open(url)
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .unwrap()
}

async fn run(scratch: &Keyspace, command: &str, arguments: &[&[u8]]) {
    let mut connection = native(scratch).await;
    let mut cmd = redis::cmd(command);
    for argument in arguments {
        cmd.arg(*argument);
    }
    cmd.query_async::<redis::Value>(&mut connection).await.unwrap();
}

impl keyspace_conformance::KeyspaceEngine for Redis {
    type Driver = RedisDriver;
    type Scratch = Keyspace;

    const DRIVER_ENFORCES_READ_ONLY: bool = false;

    fn driver() -> RedisDriver {
        RedisDriver
    }

    async fn scratch() -> Keyspace {
        let (host, port) = SERVER.get().unwrap().clone();
        let database = NEXT_DATABASE.fetch_add(1, Ordering::SeqCst);
        let scratch = Keyspace { host, port, database };
        run(&scratch, "FLUSHDB", &[]).await;
        scratch
    }

    fn options(scratch: &Keyspace, read_only: bool) -> ConnectOptions {
        ConnectOptions {
            host: scratch.host.clone(),
            port: scratch.port,
            database: scratch.database.to_string(),
            password: secrecy::SecretString::new(String::new().into()),
            tls: TlsConfig::disabled(),
            read_only,
            ..Default::default()
        }
    }

    fn browse_target(scratch: &Keyspace) -> String {
        format!("db{}", scratch.database)
    }

    fn full_value_query(scratch: &Keyspace) -> String {
        format!("SELECT \"Value\" FROM \"db{}\" WHERE \"Key\" = ?", scratch.database)
    }

    fn type_label(kind: KeyKind) -> &'static str {
        match kind {
            KeyKind::String => "string",
            KeyKind::Hash => "hash",
            KeyKind::List => "list",
            KeyKind::Set => "set",
            KeyKind::SortedSet => "zset",
            KeyKind::Stream => "stream",
        }
    }

    fn write_statements() -> Vec<String> {
        ["SET conf:key:000 changed", "DEL conf:key:001", "FLUSHDB"]
            .map(String::from)
            .to_vec()
    }

    async fn seed_string(scratch: &Keyspace, key: &[u8], value: &[u8]) {
        run(scratch, "SET", &[key, value]).await;
    }

    async fn seed_collection(scratch: &Keyspace, key: &[u8], kind: KeyKind) {
        match kind {
            KeyKind::Hash => run(scratch, "HSET", &[key, b"f1", b"v1", b"f2", b"v2"]).await,
            KeyKind::List => run(scratch, "RPUSH", &[key, b"a", b"b"]).await,
            KeyKind::Set => run(scratch, "SADD", &[key, b"a", b"b"]).await,
            KeyKind::SortedSet => run(scratch, "ZADD", &[key, b"1", b"a", b"2", b"b"]).await,
            KeyKind::Stream => run(scratch, "XADD", &[key, b"*", b"f", b"v"]).await,
            KeyKind::String => panic!("strings are seeded with seed_string"),
        }
    }

    async fn expire_in(scratch: &Keyspace, key: &[u8], seconds: u32) {
        run(scratch, "EXPIRE", &[key, seconds.to_string().as_bytes()]).await;
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn keyspace_conformance_suite_on_a_real_redis_server() {
    let image = RedisImage::default().with_tag(REDIS_TEST_TAG);
    let image = match std::env::var("BOOKIE_TEST_RUN_ID") {
        Ok(run_id) => image.with_label("com.bookie.test-run", run_id),
        Err(_) => image,
    };
    let container = image.start().await.expect("start redis container");
    let host = container.get_host().await.expect("host").to_string();
    let port = container.get_host_port_ipv4(6379).await.expect("port");
    SERVER.set((host, port)).expect("one server per run");
    keyspace_conformance::run_all::<Redis>().await;
}
