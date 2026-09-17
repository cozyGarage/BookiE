use tablepro_core::sql_syntax::SqlGrammar;
use tablepro_core::sql_syntax::script::{BatchErrorPolicy, LexicalSettings, ScriptPlan};

pub fn grammar_for(driver: &str) -> Option<SqlGrammar> {
    match driver {
        "postgres" => Some(SqlGrammar::PostgreSql),
        "mysql" => Some(SqlGrammar::MySql),
        "sqlite" => Some(SqlGrammar::Sqlite),
        "mssql" => Some(SqlGrammar::MsSql),
        "clickhouse" => Some(SqlGrammar::ClickHouse),
        _ => None,
    }
}

pub fn plan_for(text: &str, grammar: SqlGrammar) -> ScriptPlan {
    ScriptPlan::build(text, grammar, LexicalSettings::default_for(grammar))
}

#[derive(Debug, Clone)]
pub struct PlannedStatements {
    pub statements: Vec<String>,
    pub error_policy: BatchErrorPolicy,
}

pub fn script_statements(text: &str, driver: &str) -> Result<PlannedStatements, String> {
    let Some(grammar) = grammar_for(driver) else {
        return Ok(PlannedStatements {
            statements: tablepro_core::sql_lex::split_statements(text, driver),
            error_policy: BatchErrorPolicy::StopScript,
        });
    };
    let plan = plan_for(text, grammar);
    if !plan.diagnostics().is_empty() {
        return Err(crate::tr!("The script contains an unterminated quote or comment."));
    }
    if plan.batches().iter().any(|batch| batch.repeat.get() != 1) {
        return Err(crate::tr!(
            "GO repetition is not supported. Expand repeated batches explicitly before running."
        ));
    }
    let statements = plan
        .statements()
        .iter()
        .map(|statement| statement.text(text).trim().to_owned())
        .filter(|statement| !statement.is_empty())
        .collect();
    Ok(PlannedStatements {
        statements,
        error_policy: plan.batch_error_policy(),
    })
}

pub fn statement_at_cursor(text: &str, driver: &str, byte: usize) -> Option<String> {
    let Some(grammar) = grammar_for(driver) else {
        return tablepro_core::sql_lex::statement_at_cursor(text, driver, byte);
    };
    let plan = plan_for(text, grammar);
    let statement = plan.statement_at(byte)?;
    let affected = plan.diagnostics().iter().any(|diagnostic| {
        let tablepro_core::sql_syntax::script::ScriptDiagnostic::Unterminated { start, .. } = diagnostic;
        statement.range.contains(start)
    });
    if affected {
        return None;
    }
    Some(statement.text(text).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_boundaries_and_unicode_cursor_are_preserved() {
        let sql = "CREATE FUNCTION f() RETURNS int AS $$ SELECT 1; SELECT 2; $$ LANGUAGE sql; SELECT '東京'";
        let planned = script_statements(sql, "postgres").unwrap();
        assert_eq!(planned.statements.len(), 2);
        assert_eq!(planned.error_policy, BatchErrorPolicy::StopScript);
        assert_eq!(
            statement_at_cursor(sql, "postgres", sql.find("東京").unwrap()),
            Some("SELECT '東京'".into())
        );
        let mssql_planned = script_statements("SELECT 1; SELECT 2\nGO\nSELECT 3", "mssql").unwrap();
        assert_eq!(mssql_planned.statements, vec!["SELECT 1; SELECT 2", "SELECT 3"]);
        assert_eq!(mssql_planned.error_policy, BatchErrorPolicy::ContinueNextBatch);
        assert_eq!(
            script_statements(
                "DELIMITER $$\nCREATE PROCEDURE p() BEGIN SELECT 1; END$$\nDELIMITER ;",
                "mysql"
            )
            .unwrap()
            .statements,
            vec!["CREATE PROCEDURE p() BEGIN SELECT 1; END"]
        );
        assert!(script_statements("SELECT 1\nGO 2", "mssql").is_err());
        assert!(script_statements("SELECT 'unfinished", "postgres").is_err());
    }

    #[test]
    fn only_mssql_go_batches_continue_after_an_error() {
        for driver in ["postgres", "mysql", "sqlite", "clickhouse"] {
            assert_eq!(
                script_statements("SELECT 1", driver).unwrap().error_policy,
                BatchErrorPolicy::StopScript,
                "{driver}"
            );
        }
        assert_eq!(
            script_statements("SELECT 1", "mssql").unwrap().error_policy,
            BatchErrorPolicy::ContinueNextBatch
        );
        assert_eq!(
            script_statements("SELECT 1", "an-unknown-driver").unwrap().error_policy,
            BatchErrorPolicy::StopScript
        );
    }

    #[test]
    fn an_unterminated_construct_elsewhere_does_not_block_a_clean_statement_at_the_cursor() {
        let sql = "SELECT 1; SELECT 'unfinished";
        assert_eq!(
            statement_at_cursor(sql, "postgres", sql.find("SELECT 1").unwrap()),
            Some("SELECT 1".into())
        );
        assert_eq!(
            statement_at_cursor(sql, "postgres", sql.find("'unfinished").unwrap()),
            None
        );
    }
}
