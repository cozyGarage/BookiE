use std::sync::OnceLock;

use sqlparser::ast::Statement;
use sqlparser::dialect::Dialect;
use sqlparser::parser::{Parser, ParserError};

use crate::classify::dialect_for;

pub(crate) struct ParsedSql<'a> {
    sql: &'a str,
    driver_id: String,
    statements: OnceLock<Result<Vec<Statement>, ParserError>>,
}

impl<'a> ParsedSql<'a> {
    pub(crate) fn new(sql: &'a str, driver_id: &str) -> Self {
        Self {
            sql,
            driver_id: driver_id.to_owned(),
            statements: OnceLock::new(),
        }
    }

    pub(crate) fn sql(&self) -> &'a str {
        self.sql
    }

    pub(crate) fn trimmed(&self) -> &'a str {
        self.sql.trim()
    }

    pub(crate) fn dialect(&self) -> Box<dyn Dialect> {
        dialect_for(&self.driver_id)
    }

    pub(crate) fn statements(&self) -> Result<&[Statement], &ParserError> {
        self.statements
            .get_or_init(|| {
                counter::record();
                Parser::parse_sql(self.dialect().as_ref(), self.trimmed())
            })
            .as_deref()
    }
}

#[cfg(test)]
pub(crate) mod counter {
    use std::cell::Cell;

    thread_local! {
        static PARSES: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn record() {
        PARSES.with(|parses| parses.set(parses.get() + 1));
    }

    pub(crate) fn count() -> usize {
        PARSES.with(Cell::get)
    }

    pub(crate) fn reset() {
        PARSES.with(|parses| parses.set(0));
    }
}

#[cfg(not(test))]
mod counter {
    pub(super) fn record() {}
}

#[cfg(test)]
mod tests {
    use super::{ParsedSql, counter};
    use crate::catalog_effect::catalog_effect;
    use crate::classify::{analyze_parsed, analyze_statement, classify, classify_parsed};
    use crate::mask::SensitivePatterns;
    use crate::rules::shared_connection_decision;
    use crate::transaction_control::{hides_transaction_control, transaction_control_parsed};

    const DRIVERS: [&str; 5] = ["postgres", "mysql", "sqlite", "mssql", "duckdb"];

    fn parses_during(work: impl FnOnce()) -> usize {
        counter::reset();
        work();
        counter::count()
    }

    #[test]
    fn analysis_parses_each_statement_once_with_masking_and_blast_radius() {
        let masks = SensitivePatterns::new(&["email".to_owned()]);
        for driver in DRIVERS {
            for sql in [
                "SELECT email FROM users",
                "UPDATE users SET email = 'x' WHERE id = 1",
                "INSERT INTO users (id) VALUES (1), (2)",
            ] {
                assert_eq!(
                    parses_during(|| drop(analyze_statement(sql, driver, Some(&masks)))),
                    1,
                    "{driver} {sql}"
                );
                assert_eq!(parses_during(|| drop(classify(sql, driver))), 1, "{driver} {sql}");
            }
        }
    }

    #[test]
    fn transaction_control_and_analysis_share_one_parse() {
        for driver in DRIVERS {
            for sql in ["BEGIN", "COMMIT", "SELECT 1; SELECT 2", "UPDATE t SET a = 1"] {
                let parses = parses_during(|| {
                    let parsed = ParsedSql::new(sql, driver);
                    let _ = transaction_control_parsed(&parsed, driver);
                    let _ = hides_transaction_control(&parsed, driver);
                    let _ = analyze_parsed(&parsed, driver, None, true);
                });
                assert_eq!(parses, 1, "{driver} {sql}");
            }
        }
    }

    #[test]
    fn catalog_effect_parses_a_statement_once() {
        for sql in ["CREATE TABLE a (id INT)", "BEGIN", "SELECT 1", "COMMIT AND CHAIN"] {
            assert_eq!(
                parses_during(|| {
                    let _ = catalog_effect(sql, "postgres");
                }),
                1,
                "{sql}"
            );
        }
    }

    #[test]
    fn shared_connection_rules_parse_each_script_statement_once() {
        let sql = "BEGIN; INSERT INTO t VALUES (1); INSERT INTO t VALUES (2); COMMIT";
        let facts = classify("SELECT 1", "postgres");
        assert_eq!(
            parses_during(|| drop(shared_connection_decision(sql, "postgres", &facts))),
            4
        );
    }

    #[test]
    fn a_parsed_context_is_not_reparsed_when_reused() {
        let parsed = ParsedSql::new("SELECT 1", "postgres");
        let parses = parses_during(|| {
            for _ in 0..3 {
                let _ = classify_parsed(&parsed, "postgres");
            }
        });
        assert_eq!(parses, 1);
    }
}
