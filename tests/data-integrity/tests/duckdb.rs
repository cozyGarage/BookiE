#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{duckdb, judge_and_report, load_corpus, routes, runner};

#[tokio::test]
async fn duckdb_reads_match_the_engine_rendering() {
    let corpus = load_corpus("duckdb");
    let server = duckdb::start("duckdb");
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
async fn duckdb_copy_export_import_and_edits_match_the_engine() {
    let corpus = load_corpus("duckdb");
    let server = duckdb::start("duckdb");
    let results = routes::run_routes(&server, &corpus, true).await;
    judge_and_report(&corpus, &results).assert_clean();
}
