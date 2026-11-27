#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{judge_and_report, load_corpus, postgres, routes, runner};

#[tokio::test]
#[ignore = "requires docker"]
async fn postgres_workbook_null_is_refused_instead_of_written_as_empty_text() {
    let mut corpus = load_corpus("postgres");
    corpus.cases.retain(|case| case.id == "bpchar");
    assert_eq!(corpus.cases.len(), 1);
    let server = postgres::start("16-alpine", "pg16").await;
    let results = routes::run_routes(&server, &corpus, true).await;
    let import = results
        .iter()
        .find(|result| result.route == bookie_data_integrity::corpus::Route::ImportXlsx)
        .unwrap();
    assert!(
        matches!(&import.outcome, bookie_data_integrity::report::Outcome::Refused(errors) if errors.iter().any(|error| error.contains("ambiguous"))),
        "{import:?}"
    );
    judge_and_report(&corpus, &results).assert_clean();
}

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

#[tokio::test]
#[ignore = "requires docker"]
async fn postgres_16_copy_export_import_and_edits_match_the_server() {
    let corpus = load_corpus("postgres");
    let server = postgres::start("16-alpine", "pg16").await;
    let results = routes::run_routes(&server, &corpus, true).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn postgres_18_copy_export_import_and_edits_match_the_server() {
    let corpus = load_corpus("postgres");
    let server = postgres::start("18-alpine", "pg18").await;
    let results = routes::run_routes(&server, &corpus, true).await;
    judge_and_report(&corpus, &results).assert_clean();
}
