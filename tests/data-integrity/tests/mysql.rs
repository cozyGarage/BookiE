#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{judge_and_report, load_corpus, mysql, runner};

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_8_4_reads_match_the_server_rendering() {
    let corpus = load_corpus("mysql");
    let server = mysql::start_mysql("mysql-8.4").await;
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_11_reads_match_the_server_rendering() {
    let corpus = load_corpus("mysql");
    let server = mysql::start_mariadb("mariadb-11").await;
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}
