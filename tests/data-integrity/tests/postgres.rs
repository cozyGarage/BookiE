#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{judge_and_report, load_corpus, postgres, runner};

async fn read_corpus_on(tag: &str, label: &str) {
    let corpus = load_corpus("postgres");
    let server = postgres::start(tag, label).await;
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn postgres_16_reads_match_the_server_rendering() {
    read_corpus_on("16-alpine", "pg16").await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn postgres_18_reads_match_the_server_rendering() {
    read_corpus_on("18-alpine", "pg18").await;
}
