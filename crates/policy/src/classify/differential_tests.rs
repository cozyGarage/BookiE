use sqlparser::ast::Statement;

use super::legacy_oracle_tests as legacy_oracle;
use super::{StatementDetails, classify_statement};
use crate::effects::Effects;
use crate::parsed_sql::ParsedSql;

const DRIVERS: [&str; 7] = ["postgres", "mysql", "sqlite", "mssql", "duckdb", "clickhouse", "oracle"];

const LEAVES: [&str; 16] = [
    "SELECT 1",
    "SELECT id FROM accounts",
    "SELECT id FROM accounts FOR UPDATE",
    "SELECT pg_terminate_backend(1)",
    "SELECT set_config('a', 'b', false)",
    "SELECT sleep(1)",
    "SELECT writefile('x', 'y')",
    "SELECT lo_export(1, '/tmp/x')",
    "SELECT * INTO backup FROM t",
    "VALUES (1)",
    "TABLE t",
    "INSERT INTO t VALUES (1) RETURNING id",
    "UPDATE t SET a = 1 RETURNING id",
    "DELETE FROM t RETURNING id",
    "SELECT a FROM t UNION SELECT b FROM u",
    "SELECT * FROM a JOIN (SELECT pg_cancel_backend(2)) AS x ON true",
];

const WRAPPERS: [&str; 16] = [
    "SELECT * FROM ({}) AS d",
    "SELECT ({}) AS v",
    "SELECT 1 WHERE EXISTS ({})",
    "SELECT * FROM t WHERE id IN ({})",
    "SELECT coalesce(({}), 0)",
    "SELECT CAST(({}) AS text)",
    "SELECT CASE WHEN ({}) IS NULL THEN 1 ELSE ({}) END",
    "WITH c AS ({}) SELECT * FROM c",
    "SELECT * FROM a JOIN ({}) AS d ON d.id = (SELECT 1)",
    "SELECT count(*) FILTER (WHERE a IN ({})) FROM t",
    "SELECT pg_terminate_backend(({}))",
    "SELECT upper(({})) || lower(({}))",
    "SELECT * FROM a JOIN t ON t.id IN ({})",
    "SELECT max(a) FROM t GROUP BY b HAVING max(a) > ({})",
    "SELECT * FROM a, (({}) UNION ALL (SELECT 2)) AS u",
    "SELECT - ({}) AS n, (SELECT 1) BETWEEN 0 AND ({})",
];

fn summary(details: &StatementDetails) -> (super::StatementClass, Effects, Vec<String>, bool, bool) {
    (
        details.class,
        details.effects,
        details.tables.clone(),
        details.has_where,
        details.contains_unscoped_dml,
    )
}

fn assert_same(sql: &str, driver: &str) -> usize {
    let parsed = ParsedSql::new(sql, driver);
    let Ok(statements) = parsed.statements() else {
        return 0;
    };
    for statement in statements {
        assert_same_statement(statement, sql, driver);
    }
    statements.len()
}

fn assert_same_statement(statement: &Statement, sql: &str, driver: &str) {
    assert_eq!(
        summary(&classify_statement(statement)),
        summary(&legacy_oracle::classify_statement(statement)),
        "{driver}: {sql}"
    );
}

fn string_literals(source: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find('"') {
        let raw_hashes = rest[..start]
            .strip_suffix('#')
            .map_or(0, |prefix| prefix.len() - prefix.trim_end_matches('#').len() + 1);
        let body = &rest[start + 1..];
        let terminator = format!("\"{}", "#".repeat(raw_hashes));
        let end = if raw_hashes > 0 {
            body.find(&terminator)
        } else {
            body.char_indices()
                .scan(false, |escaped, (at, character)| {
                    let closes = character == '"' && !*escaped;
                    *escaped = character == '\\' && !*escaped;
                    Some((at, closes))
                })
                .find_map(|(at, closes)| closes.then_some(at))
        };
        let Some(end) = end else {
            break;
        };
        literals.push(
            body[..end]
                .replace("\\n", "\n")
                .replace("\\\"", "\"")
                .replace("\\\\", "\\"),
        );
        rest = &body[end + terminator.len()..];
    }
    literals
}

fn nested_corpus() -> Vec<String> {
    let mut corpus: Vec<String> = LEAVES.iter().map(|leaf| (*leaf).to_owned()).collect();
    let mut frontier = corpus.clone();
    for _ in 0..2 {
        frontier = frontier
            .iter()
            .flat_map(|inner| WRAPPERS.iter().map(move |wrapper| wrapper.replace("{}", inner)))
            .collect();
        corpus.extend(frontier.iter().cloned());
    }
    corpus
}

#[test]
fn the_select_walk_matches_the_legacy_helpers_on_the_recorded_corpus() {
    let corpus = include_str!("../../tests/classification_corpus.txt");
    let mut statements = 0;
    for line in corpus.lines() {
        let mut fields = line.split('|');
        let (Some(recorded_driver), Some(sql)) = (fields.next(), fields.next()) else {
            continue;
        };
        statements += assert_same(sql, recorded_driver);
        for driver in DRIVERS {
            statements += assert_same(sql, driver);
        }
    }
    assert!(statements > 500, "only {statements} statements compared");
}

#[test]
fn the_select_walk_matches_the_legacy_helpers_on_every_policy_test_literal() {
    let sources = include_str!(concat!(env!("OUT_DIR"), "/policy-sources.rs"));
    let mut statements = 0;
    for literal in string_literals(sources) {
        for driver in DRIVERS {
            statements += assert_same(&literal, driver);
        }
    }
    assert!(statements > 5_000, "only {statements} statements compared");
}

#[test]
fn the_select_walk_matches_the_legacy_helpers_on_generated_nesting() {
    let mut statements = 0;
    for sql in nested_corpus() {
        for driver in DRIVERS {
            statements += assert_same(&sql, driver);
        }
    }
    assert!(statements > 10_000, "only {statements} statements compared");
}
