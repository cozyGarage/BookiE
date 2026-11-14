use std::sync::OnceLock;

use sqlparser::ast::Statement;
use sqlparser::dialect::Dialect;
use sqlparser::parser::{Parser, ParserError};

use bookie_core::sql_syntax::SqlGrammar;
use bookie_core::sql_syntax::script::{LexicalSettings, ScriptPlan};

use crate::classify::dialect_for;

pub(crate) struct ParsedSql<'a> {
    sql: &'a str,
    driver_id: String,
    lexical_mode_ambiguous: bool,
    statements: OnceLock<Result<Vec<Statement>, ParserError>>,
}

impl<'a> ParsedSql<'a> {
    pub(crate) fn new(sql: &'a str, driver_id: &str) -> Self {
        Self::with_lexical_settings(sql, driver_id, None)
    }

    pub(crate) fn with_lexical_settings(sql: &'a str, driver_id: &str, settings: Option<LexicalSettings>) -> Self {
        Self {
            sql,
            driver_id: driver_id.to_owned(),
            lexical_mode_ambiguous: lexical_mode_ambiguous(sql, driver_id, settings),
            statements: OnceLock::new(),
        }
    }

    pub(crate) fn lexical_mode_ambiguous(&self) -> bool {
        self.lexical_mode_ambiguous
    }

    pub(crate) fn needs_lexical_settings(sql: &str, driver_id: &str) -> bool {
        match driver_id {
            "mysql" => sql.bytes().any(|byte| matches!(byte, b'\\' | b'"')),
            "postgres" => sql.as_bytes().contains(&b'\\'),
            _ => false,
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

fn lexical_mode_ambiguous(sql: &str, driver_id: &str, actual: Option<LexicalSettings>) -> bool {
    let grammar = match driver_id {
        "mysql" if ParsedSql::needs_lexical_settings(sql, driver_id) => SqlGrammar::MySql,
        "postgres" if ParsedSql::needs_lexical_settings(sql, driver_id) => SqlGrammar::PostgreSql,
        _ => return false,
    };
    let default = LexicalSettings::default_for(grammar);
    let reference = ScriptPlan::build(sql, grammar, default)
        .tokens(0..sql.len())
        .collect::<Vec<_>>();
    if let Some(actual) = actual {
        return actual != default
            && ScriptPlan::build(sql, grammar, actual)
                .tokens(0..sql.len())
                .ne(reference.iter().cloned());
    }
    LexicalSettings::ALL
        .into_iter()
        .filter(|settings| grammar != SqlGrammar::PostgreSql || !settings.ansi_quotes)
        .filter(|settings| *settings != default)
        .any(|settings| {
            ScriptPlan::build(sql, grammar, settings)
                .tokens(0..sql.len())
                .ne(reference.iter().cloned())
        })
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
    use super::{ParsedSql, counter, lexical_mode_ambiguous};
    use crate::catalog_effect::catalog_effect;
    use crate::classify::{analyze_parsed, analyze_statement, classify, classify_parsed};
    use crate::mask::SensitivePatterns;
    use crate::rules::shared_connection_decision;
    use crate::transaction_control::{hides_transaction_control, transaction_control_parsed};
    use bookie_core::sql_syntax::SqlGrammar;
    use bookie_core::sql_syntax::script::LexicalSettings;

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

    #[test]
    fn lexical_ambiguity_only_compares_modes_supported_by_the_engine() {
        let mysql = LexicalSettings::default_for(SqlGrammar::MySql);
        let postgres = LexicalSettings::default_for(SqlGrammar::PostgreSql);
        let no_backslash = LexicalSettings::from_mysql_sql_mode("NO_BACKSLASH_ESCAPES");
        let postgres_off = LexicalSettings::from_postgres_standard_conforming_strings("off");
        let sql = r#"SELECT 'x\'; DELETE FROM users"#;
        assert!(lexical_mode_ambiguous(sql, "mysql", None));
        assert!(lexical_mode_ambiguous(sql, "postgres", None));
        assert!(!lexical_mode_ambiguous(sql, "mysql", Some(mysql)));
        assert!(!lexical_mode_ambiguous(sql, "postgres", Some(postgres)));
        assert!(lexical_mode_ambiguous(sql, "mysql", Some(no_backslash)));
        assert!(lexical_mode_ambiguous(sql, "postgres", postgres_off));
        assert!(!lexical_mode_ambiguous("SELECT 'ordinary'", "mysql", None));
        assert!(!lexical_mode_ambiguous("SELECT 'ordinary'", "postgres", None));
        assert!(!lexical_mode_ambiguous("SELECT 1", "clickhouse", None));
    }
}
