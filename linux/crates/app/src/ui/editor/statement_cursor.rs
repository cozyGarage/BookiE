use tablepro_core::sql_syntax::SqlGrammar;
use tablepro_core::sql_syntax::script::{LexicalSettings, ScriptPlan};

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

pub fn script_statements(text: &str, driver: &str) -> Result<Vec<String>, String> {
    let Some(grammar) = grammar_for(driver) else {
        return Ok(tablepro_core::sql_lex::split_statements(text, driver));
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
    Ok(plan
        .statements()
        .iter()
        .map(|statement| statement.text(text).trim().to_owned())
        .filter(|statement| !statement.is_empty())
        .collect())
}

pub fn statement_at_cursor(text: &str, driver: &str, byte: usize) -> Option<String> {
    let Some(grammar) = grammar_for(driver) else {
        return tablepro_core::sql_lex::statement_at_cursor(text, driver, byte);
    };
    let plan = plan_for(text, grammar);
    if !plan.diagnostics().is_empty() {
        return None;
    }
    plan.statement_at(byte)
        .map(|statement| statement.text(text).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_boundaries_and_unicode_cursor_are_preserved() {
        let sql = "CREATE FUNCTION f() RETURNS int AS $$ SELECT 1; SELECT 2; $$ LANGUAGE sql; SELECT '東京'";
        let statements = script_statements(sql, "postgres").unwrap();
        assert_eq!(statements.len(), 2);
        assert_eq!(
            statement_at_cursor(sql, "postgres", sql.find("東京").unwrap()),
            Some("SELECT '東京'".into())
        );
        assert_eq!(
            script_statements("SELECT 1; SELECT 2\nGO\nSELECT 3", "mssql").unwrap(),
            vec!["SELECT 1; SELECT 2", "SELECT 3"]
        );
        assert_eq!(
            script_statements(
                "DELIMITER $$\nCREATE PROCEDURE p() BEGIN SELECT 1; END$$\nDELIMITER ;",
                "mysql"
            )
            .unwrap(),
            vec!["CREATE PROCEDURE p() BEGIN SELECT 1; END"]
        );
        assert!(script_statements("SELECT 1\nGO 2", "mssql").is_err());
        assert!(script_statements("SELECT 'unfinished", "postgres").is_err());
    }
}
