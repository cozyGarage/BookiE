use tablepro_core::sql_syntax::SqlGrammar;
use tablepro_core::sql_syntax::script::{LexicalSettings, ScriptPlan};

pub(crate) fn script_statements(sql: &str, driver_id: &str) -> Vec<String> {
    let Some(grammar) = compound_body_grammar(driver_id) else {
        return tablepro_core::sql_lex::split_statements(sql, driver_id);
    };
    ScriptPlan::build(sql, grammar, LexicalSettings::default_for(grammar))
        .statements()
        .iter()
        .map(|statement| statement.text(sql).trim())
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .collect()
}

fn compound_body_grammar(driver_id: &str) -> Option<SqlGrammar> {
    match driver_id {
        "sqlite" => Some(SqlGrammar::Sqlite),
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
    fn the_other_drivers_keep_the_plain_splitter() {
        for driver in ["postgres", "mysql", "mssql", "clickhouse", "duckdb", "redis"] {
            assert_eq!(
                script_statements("SELECT 1; BEGIN", driver),
                tablepro_core::sql_lex::split_statements("SELECT 1; BEGIN", driver),
                "{driver}"
            );
        }
    }
}
