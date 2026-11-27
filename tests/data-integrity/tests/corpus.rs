#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{corpus, load_corpus, redis};

#[test]
fn every_corpus_file_parses_and_names_an_issue_for_each_known_failure() {
    for engine in corpus::ENGINES {
        let corpus = load_corpus(engine);
        assert_eq!(corpus.engine, engine);
        assert!(!corpus.cases.is_empty(), "{engine} corpus is empty");
        let problems = corpus::validate(&corpus);
        assert!(
            problems.is_empty(),
            "{engine} corpus problems:\n{}",
            problems.join("\n")
        );
    }
}

#[test]
fn the_redis_corpus_parses_and_names_an_issue_for_each_known_failure() {
    let corpus = redis::load();
    assert_eq!(corpus.engine, "redis");
    let problems = redis::validate(&corpus);
    assert!(problems.is_empty(), "redis corpus problems:\n{}", problems.join("\n"));
}

#[test]
fn every_corpus_file_belongs_to_an_engine_runner() {
    for entry in std::fs::read_dir(corpus::source_dir()).unwrap() {
        let path = entry.unwrap().path();
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        assert!(
            corpus::ENGINES.contains(&stem.as_str()) || stem == "redis",
            "{} has no engine runner",
            path.display()
        );
    }
}
