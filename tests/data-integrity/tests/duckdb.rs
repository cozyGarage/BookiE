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

#[tokio::test]
async fn duckdb_bit_values_survive_reads_and_write_routes() {
    let mut corpus = load_corpus("duckdb");
    corpus.cases.retain(|case| case.id == "bit");
    assert_eq!(corpus.cases.len(), 1);
    let server = duckdb::start("duckdb");
    let reads = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &reads).assert_clean();
    let routes = routes::run_routes(&server, &corpus, true).await;
    judge_and_report(&corpus, &routes).assert_clean();
}

#[tokio::test]
async fn duckdb_timetz_corpus_reports_unsupported_values() {
    let mut corpus = load_corpus("duckdb");
    corpus.cases.retain(|case| case.id == "timetz");
    assert_eq!(corpus.cases.len(), 1);
    let server = duckdb::start("duckdb");
    let reads = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &reads).assert_clean();
    let routes = routes::run_routes(&server, &corpus, true).await;
    judge_and_report(&corpus, &routes).assert_clean();
}
