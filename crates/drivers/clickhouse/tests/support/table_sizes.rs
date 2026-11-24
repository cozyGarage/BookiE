#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::{connect, start_clickhouse};

#[tokio::test]
#[ignore = "requires docker"]
async fn table_sizes_sum_active_parts_for_the_current_or_named_database() {
    let (_c, opts) = start_clickhouse().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE events (id UInt64, body String) ENGINE = MergeTree ORDER BY id")
        .await
        .unwrap();
    conn.execute("INSERT INTO events SELECT number, repeat('x', 100) FROM numbers(20000)")
        .await
        .unwrap();
    conn.execute("CREATE DATABASE other_db").await.unwrap();
    conn.execute("CREATE TABLE other_db.logs (id UInt64) ENGINE = MergeTree ORDER BY id")
        .await
        .unwrap();
    conn.execute("INSERT INTO other_db.logs SELECT number FROM numbers(10)")
        .await
        .unwrap();

    let current = conn.table_sizes(None).await.unwrap();
    let events = current
        .iter()
        .find(|size| size.name == "events")
        .expect("events is listed");
    assert!(events.bytes > 0, "{events:?}");
    assert!(current.iter().all(|size| size.name != "logs"), "{current:?}");

    let other = conn.table_sizes(Some("other_db")).await.unwrap();
    assert_eq!(other.len(), 1, "{other:?}");
    assert_eq!(other[0].schema.as_deref(), Some("other_db"));
}
