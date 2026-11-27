#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{clickhouse, judge_and_report, load_corpus, routes, runner};

#[tokio::test]
#[ignore = "requires docker"]
async fn clickhouse_24_8_new_york_reads_match_the_server_rendering() {
    let corpus = load_corpus("clickhouse");
    let server = clickhouse::start("clickhouse-24.8-new-york", Some("America/New_York")).await;
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn clickhouse_24_8_reads_match_the_server_rendering() {
    let corpus = load_corpus("clickhouse");
    let server = clickhouse::start("clickhouse-24.8", None).await;
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn clickhouse_24_8_copy_export_and_import_match_the_server() {
    let corpus = load_corpus("clickhouse");
    let server = clickhouse::start("clickhouse-24.8", None).await;
    let results = routes::run_routes(&server, &corpus, false).await;
    judge_and_report(&corpus, &results).assert_clean();
}
