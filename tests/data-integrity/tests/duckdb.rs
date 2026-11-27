#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{duckdb, judge_and_report, load_corpus, runner};

#[tokio::test]
async fn duckdb_reads_match_the_engine_rendering() {
    let corpus = load_corpus("duckdb");
    let server = duckdb::start("duckdb");
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}
