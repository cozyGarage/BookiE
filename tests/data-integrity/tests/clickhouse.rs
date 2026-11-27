#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{clickhouse, judge_and_report, load_corpus, runner};

#[tokio::test]
#[ignore = "requires docker"]
async fn clickhouse_24_8_reads_match_the_server_rendering() {
    let corpus = load_corpus("clickhouse");
    let server = clickhouse::start("clickhouse-24.8").await;
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}
