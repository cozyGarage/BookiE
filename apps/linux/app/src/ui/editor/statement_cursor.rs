use tablepro_core::sql_syntax::SqlGrammar;
use tablepro_core::sql_syntax::script::{BatchErrorPolicy, LexicalSettings, ScriptPlan};

pub fn grammar_for(driver: &str) -> Option<SqlGrammar> {
    match driver {
        "postgres" => Some(SqlGrammar::PostgreSql),
        "mysql" => Some(SqlGrammar::MySql),
        "sqlite" => Some(SqlGrammar::Sqlite),
        "mssql" => Some(SqlGrammar::MsSql),
        "clickhouse" => Some(SqlGrammar::ClickHouse),
        "duckdb" => Some(SqlGrammar::PostgreSql),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Lexing<'a> {
    pub driver: &'a str,
    pub settings: Option<LexicalSettings>,
}

impl<'a> From<&'a str> for Lexing<'a> {
    fn from(driver: &'a str) -> Self {
        Self { driver, settings: None }
    }
}

impl Lexing<'_> {
    pub fn settings_for(&self, grammar: SqlGrammar) -> LexicalSettings {
        self.settings.unwrap_or_else(|| LexicalSettings::default_for(grammar))
    }

    fn plan(&self, text: &str) -> Option<ScriptPlan> {
        let grammar = grammar_for(self.driver)?;
        Some(ScriptPlan::build(text, grammar, self.settings_for(grammar)))
    }
}

pub fn cursor_byte_offset(text: &str, character_offset: usize) -> usize {
    text.chars().take(character_offset).map(char::len_utf8).sum()
}

#[derive(Debug, Clone)]
pub struct PlannedStatements {
    pub statements: Vec<String>,
    pub source_ranges: Vec<std::ops::Range<usize>>,
    pub error_policy: BatchErrorPolicy,
    pub settings_check: Option<LexicalSettings>,
}

pub fn script_statements<'a>(text: &str, lexing: impl Into<Lexing<'a>>) -> Result<PlannedStatements, String> {
    let lexing = lexing.into();
    let Some(grammar) = grammar_for(lexing.driver) else {
        return Ok(PlannedStatements {
            statements: tablepro_core::sql_lex::split_statements(text, lexing.driver),
            source_ranges: Vec::new(),
            error_policy: BatchErrorPolicy::StopScript,
            settings_check: None,
        });
    };
    let settings = lexing.settings_for(grammar);
    let plan = ScriptPlan::build(text, grammar, settings);
    if !plan.diagnostics().is_empty() {
        return Err(crate::tr!("The script contains an unterminated quote or comment."));
    }
    if plan.batches().iter().any(|batch| batch.repeat.get() != 1) {
        return Err(crate::tr!(
            "GO repetition is not supported. Expand repeated batches explicitly before running."
        ));
    }
    let mut statements = Vec::new();
    let mut source_ranges = Vec::new();
    for statement in plan.statements() {
        let original = statement.text(text);
        let trimmed = original.trim();
        if trimmed.is_empty() {
            continue;
        }
        let start = statement.range.start + original.len() - original.trim_start().len();
        source_ranges.push(start..start + trimmed.len());
        statements.push(trimmed.to_string());
    }
    let settings_dependent = statements.len() > 1
        && LexicalSettings::ALL
            .iter()
            .any(|other| ScriptPlan::build(text, grammar, *other).statements() != plan.statements());
    Ok(PlannedStatements {
        statements,
        source_ranges,
        error_policy: plan.batch_error_policy(),
        settings_check: settings_dependent.then_some(settings),
    })
}

pub fn statement_at_cursor<'a>(text: &str, lexing: impl Into<Lexing<'a>>, byte: usize) -> Option<String> {
    let lexing = lexing.into();
    let Some(plan) = lexing.plan(text) else {
        return tablepro_core::sql_lex::statement_at_cursor(text, lexing.driver, byte);
    };
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

pub fn statement_range_at<'a>(
    text: &str,
    lexing: impl Into<Lexing<'a>>,
    byte: usize,
) -> Option<std::ops::Range<usize>> {
    let plan = lexing.into().plan(text)?;
    let statement = plan.statement_at(byte)?;
    let unterminated = plan.diagnostics().iter().any(|diagnostic| {
        let tablepro_core::sql_syntax::script::ScriptDiagnostic::Unterminated { start, .. } = diagnostic;
        statement.range.contains(start)
    });
    if unterminated {
        return None;
    }
    let body = statement.text(text);
    let leading = body.len() - body.trim_start().len();
    let trimmed = body.trim();
    (!trimmed.is_empty()).then(|| statement.range.start + leading..statement.range.start + leading + trimmed.len())
}

pub fn adjacent_statement_start<'a>(
    text: &str,
    lexing: impl Into<Lexing<'a>>,
    byte: usize,
    forward: bool,
) -> Option<usize> {
    let plan = lexing.into().plan(text)?;
    let mut starts = plan.statements().iter().filter_map(|statement| {
        let body = statement.text(text);
        let trimmed = body.trim_start();
        (!trimmed.trim_end().is_empty()).then(|| statement.range.start + body.len() - trimmed.len())
    });
    if forward {
        starts.find(|start| *start > byte)
    } else {
        starts.rfind(|start| *start < byte)
    }
}

#[cfg(test)]
#[path = "statement_cursor_tests.rs"]
mod tests;
