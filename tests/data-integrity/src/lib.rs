pub mod artefact;
pub mod clickhouse;
pub mod corpus;
pub mod duckdb;
pub mod governed;
pub mod mysql;
pub mod postgres;
pub mod redis;
pub mod redis_reply;
pub mod report;
pub mod routes;
pub mod rules;
pub mod runner;
pub mod server;
pub mod sqlite;

use corpus::Corpus;
use report::{CaseResult, Verdict};

pub fn load_corpus(engine: &str) -> Corpus {
    let quote_text = match engine {
        "postgres" => postgres::quote_text,
        "mysql" => mysql::quote_text,
        "sqlite" => sqlite::quote_text,
        "duckdb" => duckdb::quote_text,
        "clickhouse" => clickhouse::quote_text,
        other => panic!("no SQL text quoting for {other}"),
    };
    corpus::load(engine, quote_text)
}

pub fn judge_and_report(corpus: &Corpus, results: &[CaseResult]) -> Verdict {
    let verdict = report::judge(corpus, results);
    println!("{}", verdict.summary());
    verdict
}
