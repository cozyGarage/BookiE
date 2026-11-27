#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{judge_and_report, load_corpus, routes, runner, sqlite};

#[tokio::test]
async fn sqlite_reads_match_the_engine_rendering() {
    let corpus = load_corpus("sqlite");
    let server = sqlite::start("sqlite");
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
async fn sqlite_copy_export_import_and_edits_match_the_engine() {
    let corpus = load_corpus("sqlite");
    let server = sqlite::start("sqlite");
    let results = routes::run_routes(&server, &corpus, true).await;
    judge_and_report(&corpus, &results).assert_clean();
}
