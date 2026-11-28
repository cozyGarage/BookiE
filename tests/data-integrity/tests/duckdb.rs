#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{duckdb, judge_and_report, load_corpus, routes, runner};

#[tokio::test]
async fn duckdb_workbook_null_is_refused_before_import() {
    let mut corpus = load_corpus("duckdb");
    corpus.cases.retain(|case| case.id == "enum");
    assert_eq!(corpus.cases.len(), 1);
    let server = duckdb::start("duckdb");
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
async fn duckdb_union_import_refuses_diagnostic_markers() {
    let mut corpus = load_corpus("duckdb");
    corpus.cases.retain(|case| case.id == "union");
    assert_eq!(corpus.cases.len(), 1);
    let server = duckdb::start("duckdb");
    let results = routes::run_routes(&server, &corpus, true).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
async fn duckdb_json_reads_keep_exact_source_text() {
    let mut corpus = load_corpus("duckdb");
    corpus.cases.retain(|case| case.id == "json");
    assert_eq!(corpus.cases.len(), 1);
    let server = duckdb::start("duckdb");
    let reads = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &reads).assert_clean();
}

#[tokio::test]
async fn duckdb_timetz_corpus_preserves_exact_values_across_routes() {
    let mut corpus = load_corpus("duckdb");
    corpus.cases.retain(|case| case.id == "timetz");
    assert_eq!(corpus.cases.len(), 1);
    let server = duckdb::start("duckdb");
    let reads = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &reads).assert_clean();
    let routes = routes::run_routes(&server, &corpus, true).await;
    judge_and_report(&corpus, &routes).assert_clean();
}
