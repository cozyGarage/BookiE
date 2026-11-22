#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_policy::classify;

#[path = "support/golden.rs"]
mod golden;

fn describe(driver: &str, sql: &str) -> String {
    let facts = classify(sql, driver);
    format!(
        "{driver}|{sql}|{:?}|{}|{}|{}|{}|{}",
        facts.class,
        facts.tables.join(","),
        facts.has_where,
        facts.contains_unscoped_dml,
        facts.is_multi_statement,
        facts.parse_error.is_some()
    )
}

#[test]
fn the_statement_corpus_classifies_as_recorded() {
    let path = std::path::PathBuf::from("tests/classification_corpus.txt");
    let corpus = std::fs::read_to_string(&path).expect("classification corpus");
    let actual: Vec<String> = corpus
        .lines()
        .map(|line| {
            let mut fields = line.split('|');
            let driver = fields.next().expect("driver");
            let sql = fields.next().expect("SQL");
            describe(driver, sql)
        })
        .collect();
    golden::assert_golden(&path, &actual);
}

#[test]
fn prebuilt_corpus_test_does_not_need_cargo_environment() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .env_remove("CARGO_MANIFEST_DIR")
        .env_remove("BOOKIE_RECORD")
        .arg("--exact")
        .arg("the_statement_corpus_classifies_as_recorded")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
}

#[test]
fn every_recorded_write_class_is_still_refused_as_read_only() {
    for line in include_str!("classification_corpus.txt").lines() {
        let fields: Vec<&str> = line.split('|').collect();
        let class = fields[fields.len() - 6];
        let writes = classify(fields[1], fields[0]).writes();
        let expected = !matches!(class, "Select" | "Transaction");
        assert_eq!(writes, expected, "{line}");
    }
}
