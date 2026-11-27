use std::collections::BTreeMap;

use bookie_core::Value;

use super::*;
use crate::corpus::{self, Corpus};
use crate::rules::RefereeColumn;
use crate::runner::{RefereeRow, compare_rows};

fn corpus(text: &str) -> Corpus {
    toml::from_str(text).unwrap()
}

const BASE: &str = r#"
engine = "mysql"
servers = ["a", "b"]

[[case]]
id = "one"
type = "INT"
literals = ["1", "2"]
carrier = "Int"
rule = "exact"
known_failing = [{ route = "param_select", issue = 7, expect = "carrier Bool", reason = "r" }]

[[case]]
id = "two"
type = "INT"
literals = ["1"]
carrier = "Int"
rule = "exact"

[[case]]
id = "three"
type = "INT"
literals = ["1"]
carrier = "Int"
rule = "exact"

[[known_failing]]
cases = ["two", "three"]
route = "browse"
issue = 9
expect = "BookiE has Undecodable"
servers = ["b"]
reason = "r"
"#;

fn result(case: &str, server: &str, route: Route, outcome: Outcome) -> CaseResult {
    CaseResult {
        server: server.into(),
        case: case.into(),
        route,
        outcome,
    }
}

fn fail(detail: &str) -> Outcome {
    Outcome::Fail(vec![detail.into()])
}

#[test]
fn a_known_failure_counts_only_when_the_failure_matches_its_expectation() {
    let corpus = corpus(BASE);
    let matching = result(
        "one",
        "a",
        Route::ParamSelect,
        fail("id 1 (1): expected carrier Bool, BookiE has Int(1)"),
    );
    let other = result(
        "one",
        "a",
        Route::ParamSelect,
        fail("id 1 (1): server \"1\", BookiE \"2\""),
    );
    let verdict = judge(&corpus, &[matching, other]);
    assert_eq!(verdict.tallies["a"].known_failing, 1);
    assert_eq!(verdict.tallies["a"].failed, 1);
    assert_eq!(verdict.problems.len(), 1);
}

#[test]
fn a_known_failure_that_passes_is_reported_as_fixed() {
    let corpus = corpus(BASE);
    let verdict = judge(
        &corpus,
        &[result("one", "b", Route::ParamSelect, Outcome::Pass { rows: 2 })],
    );
    assert_eq!(verdict.tallies["b"].fixed, 1);
    assert_eq!(verdict.problems.len(), 1);
}

#[test]
fn a_corpus_level_marker_covers_only_its_cases_route_and_servers() {
    let corpus = corpus(BASE);
    let failure = || fail("id 1 (1): expected carrier Int, BookiE has Undecodable(\"X\")");
    let verdict = judge(
        &corpus,
        &[
            result("two", "b", Route::Browse, failure()),
            result("three", "b", Route::Browse, failure()),
            result("two", "a", Route::Browse, failure()),
            result("two", "b", Route::Query, failure()),
            result("one", "b", Route::Browse, failure()),
        ],
    );
    assert_eq!(verdict.tallies["b"].known_failing, 2);
    assert_eq!(verdict.tallies["b"].failed, 2);
    assert_eq!(verdict.tallies["a"].failed, 1);
}

#[test]
fn a_marker_with_row_ids_must_name_exactly_the_failing_rows() {
    let text = BASE.replace("expect = \"carrier Bool\",", "expect = \"carrier Bool\", ids = [2],");
    let corpus = corpus(&text);
    let wrong_row = result("one", "a", Route::ParamSelect, fail("id 1 (1): expected carrier Bool"));
    let right_row = result("one", "b", Route::ParamSelect, fail("id 2 (2): expected carrier Bool"));
    let verdict = judge(&corpus, &[wrong_row, right_row]);
    assert_eq!(verdict.tallies["a"].failed, 1);
    assert_eq!(verdict.tallies["b"].known_failing, 1);
}

#[test]
fn a_broken_case_is_never_a_known_failure() {
    let corpus = corpus(BASE);
    let verdict = judge(
        &corpus,
        &[result(
            "one",
            "a",
            Route::ParamSelect,
            Outcome::Broken("carrier Bool".into()),
        )],
    );
    assert_eq!(verdict.tallies["a"].failed, 1);
}

#[test]
fn validation_rejects_markers_without_an_expectation_and_unknown_corpus_cases() {
    let text = BASE
        .replace("expect = \"carrier Bool\",", "expect = \" \",")
        .replace("cases = [\"two\", \"three\"]", "cases = [\"two\", \"four\"]");
    let problems = corpus::validate(&corpus(&text));
    assert!(
        problems.iter().any(|problem| problem.contains("expect")),
        "{problems:?}"
    );
    assert!(
        problems.iter().any(|problem| problem.contains("unknown case four")),
        "{problems:?}"
    );
}

#[test]
fn validation_rejects_server_cast_on_a_type_with_a_modifier() {
    let text = r#"
engine = "postgres"
servers = ["a"]

[[case]]
id = "cast"
type = "numeric(10,2)"
literals = ["1"]
carrier = "Text"
rule = "server_cast"
note = "n"

[[case]]
id = "array"
type = "integer[]"
literals = ["'{1}'"]
carrier = "Text"
rule = "server_cast"
"#;
    let problems = corpus::validate(&corpus(text));
    assert!(
        problems
            .iter()
            .any(|problem| problem.starts_with("cast: server_cast cannot")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|problem| problem.starts_with("array: a server_cast case needs a note")),
        "{problems:?}"
    );
    assert_eq!(problems.len(), 2, "{problems:?}");
}

fn referee_row(id: i64, text: &str) -> RefereeRow {
    RefereeRow {
        id,
        type_name: None,
        columns: BTreeMap::from([(RefereeColumn::Text, Some(text.to_string()))]),
    }
}

#[test]
fn a_referee_with_fewer_rows_than_literals_breaks_the_case() {
    let corpus = corpus(BASE);
    let case = &corpus.cases[0];
    let empty = compare_rows(case, Route::Query, &[], Ok(BTreeMap::new()));
    assert!(matches!(empty, Outcome::Broken(_)), "{empty:?}");
    let observed = BTreeMap::from([(1, Value::Int(1)), (2, Value::Int(2))]);
    let full = compare_rows(
        case,
        Route::Query,
        &[referee_row(1, "1"), referee_row(2, "2")],
        Ok(observed),
    );
    assert!(matches!(full, Outcome::Pass { rows: 2 }), "{full:?}");
}
