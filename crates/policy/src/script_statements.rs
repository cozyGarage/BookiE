use bookie_core::sql_syntax::SqlGrammar;
use bookie_core::sql_syntax::script::{LexicalSettings, ScriptPlan};

pub(crate) fn script_statements(sql: &str, driver_id: &str) -> Vec<String> {
    if driver_id == "mssql" && !is_mssql_routine_definition(sql) {
        return bookie_core::sql_lex::split_statements(sql, driver_id);
    }
    let Some(grammar) = script_grammar(driver_id) else {
        return bookie_core::sql_lex::split_statements(sql, driver_id);
    };
    ScriptPlan::build(sql, grammar, LexicalSettings::default_for(grammar))
        .statements()
        .iter()
        .map(|statement| statement.text(sql).trim())
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .collect()
}

fn is_mssql_routine_definition(sql: &str) -> bool {
    let parsed = crate::parsed_sql::ParsedSql::new(sql, "mssql");
    parsed.statements().is_ok_and(|statements| {
        matches!(
            statements.first(),
            Some(sqlparser::ast::Statement::CreateProcedure { .. })
        )
    })
}

fn script_grammar(driver_id: &str) -> Option<SqlGrammar> {
    match driver_id {
        "postgres" => Some(SqlGrammar::PostgreSql),
        "mysql" => Some(SqlGrammar::MySql),
        "sqlite" => Some(SqlGrammar::Sqlite),
        "mssql" => Some(SqlGrammar::MsSql),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::classify;
    use crate::rules::shared_connection_decision;

    const TRIGGER: &str = "CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE b SET n = n + 1; END; SELECT 1;";

    #[test]
    fn a_trigger_body_stays_in_one_statement() {
        let statements = script_statements(TRIGGER, "sqlite");

        assert_eq!(statements.len(), 2, "{statements:?}");
        assert!(statements[0].starts_with("CREATE TRIGGER"), "{statements:?}");
        assert!(statements[0].contains("END"), "{statements:?}");
        assert!(statements[1].starts_with("SELECT 1"), "{statements:?}");
    }

    #[test]
    fn a_trigger_script_is_not_mistaken_for_transaction_control() {
        let decision = shared_connection_decision(TRIGGER, "sqlite", &classify(TRIGGER, "sqlite"));

        assert!(decision.is_none(), "{decision:?}");
    }

    #[test]
    fn transaction_control_in_a_batch_is_still_judged_statement_by_statement() {
        let closed = "BEGIN; UPDATE x SET a = 1; COMMIT";
        let open = "BEGIN; UPDATE x SET a = 1";

        assert!(shared_connection_decision(closed, "sqlite", &classify(closed, "sqlite")).is_none());
        let refused = shared_connection_decision(open, "sqlite", &classify(open, "sqlite"));
        assert_eq!(
            refused.map(|decision| decision.rule_name().to_string()).as_deref(),
            Some("transaction_control_needs_session")
        );
    }

    #[test]
    fn transaction_control_after_a_trigger_is_still_refused_on_a_shared_connection() {
        let trigger = "CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE b SET n = n + 1; END";
        for sql in [
            format!("{trigger}; BEGIN; UPDATE x SET a = 1"),
            format!("{trigger}; COMMIT"),
            format!("{trigger}; BEGIN; BEGIN"),
        ] {
            let decision = shared_connection_decision(&sql, "sqlite", &classify(&sql, "sqlite"));
            assert_eq!(
                decision.map(|decision| decision.rule_name().to_string()).as_deref(),
                Some("transaction_control_needs_session"),
                "{sql}"
            );
        }
    }

    #[test]
    fn a_whole_transaction_around_a_trigger_is_accepted_as_one_batch() {
        let sql = "BEGIN; CREATE TRIGGER t AFTER INSERT ON a BEGIN SELECT 1; END; COMMIT";

        assert!(shared_connection_decision(sql, "sqlite", &classify(sql, "sqlite")).is_none());
    }

    #[test]
    fn routine_bodies_remain_one_statement_for_their_dialect() {
        let mut failures = Vec::new();
        for (driver, sql) in [
            ("mysql", "CREATE PROCEDURE p() BEGIN SELECT 1; END; SELECT 2;"),
            (
                "postgres",
                "CREATE FUNCTION f() RETURNS integer LANGUAGE SQL BEGIN ATOMIC SELECT 1; END; SELECT 2;",
            ),
            ("mssql", "CREATE PROCEDURE p AS BEGIN SELECT 1; END;"),
        ] {
            let statements = script_statements(sql, driver);

            if statements.len() != if driver == "mssql" { 1 } else { 2 }
                || !statements[0].starts_with("CREATE ")
                || !statements[0].contains("END")
            {
                failures.push(format!("{driver}: {statements:?}"));
            }
        }
        assert!(failures.is_empty(), "{failures:?}");
    }

    #[test]
    fn routine_ddl_is_not_mistaken_for_an_unclosed_shared_transaction() {
        for (driver, sql) in [
            ("mysql", "CREATE PROCEDURE p() BEGIN SELECT 1; END; SELECT 2;"),
            (
                "postgres",
                "CREATE FUNCTION f() RETURNS integer LANGUAGE SQL BEGIN ATOMIC SELECT 1; END; SELECT 2;",
            ),
            ("mssql", "CREATE PROCEDURE p AS BEGIN SELECT 1; END;"),
        ] {
            let decision = shared_connection_decision(sql, driver, &classify(sql, driver));

            assert!(decision.is_none(), "{driver}: {decision:?}");
        }
    }

    #[test]
    fn drivers_without_a_script_grammar_keep_the_plain_splitter() {
        for driver in ["clickhouse", "duckdb", "redis"] {
            assert_eq!(
                script_statements("SELECT 1; BEGIN", driver),
                bookie_core::sql_lex::split_statements("SELECT 1; BEGIN", driver),
                "{driver}"
            );
        }
    }

    #[test]
    fn explicit_tsql_transaction_checks_survive_batch_planning() {
        for (sql, driver, denied) in [
            ("BEGIN TRAN; UPDATE t SET n = 1", "mssql", true),
            ("BEGIN TRAN; UPDATE t SET n = 1; COMMIT", "mssql", false),
            ("BEGIN; SELECT 1", "postgres", true),
            ("BEGIN; SELECT 1; COMMIT", "postgres", false),
            ("START TRANSACTION; SELECT 1", "mysql", true),
            ("START TRANSACTION; SELECT 1; COMMIT", "mysql", false),
        ] {
            let decision = shared_connection_decision(sql, driver, &classify(sql, driver));
            assert_eq!(decision.is_some(), denied, "{driver}: {sql}: {decision:?}");
        }
    }
}
