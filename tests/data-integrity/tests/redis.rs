#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::redis;

#[tokio::test]
#[ignore = "requires docker"]
async fn redis_7_4_reads_match_the_server_replies() {
    let corpus = redis::load();
    let server = redis::start("redis-7.4").await;
    let results = redis::run(&server, &corpus).await;
    redis::judge(&corpus, &results).assert_clean();
}
