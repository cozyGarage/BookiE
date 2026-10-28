use std::ops::Range;

use tablepro_core::sql_syntax::SqlGrammar;
use tablepro_core::sql_syntax::script::{
    LexicalSettings, OpenConstruct, ScriptDiagnostic, ScriptPlan, ScriptTokenKind,
};

use super::completion::{SchemaIndex, Scope, candidate_sql, scope_at};
use super::completion_identifiers::{normalized, plan_with, split_table_reference};

pub struct Candidates {
    pub range: Range<usize>,
    pub values: Vec<(String, String)>,
}

pub fn candidates(sql: &str, cursor: usize, index: &SchemaIndex) -> Candidates {
    candidates_with(sql, cursor, index, None)
}

pub fn candidates_with(sql: &str, cursor: usize, index: &SchemaIndex, settings: Option<LexicalSettings>) -> Candidates {
    let grammar = index.grammar.unwrap_or(SqlGrammar::PostgreSql);
    let settings_for_grammar = settings.unwrap_or_else(|| LexicalSettings::default_for(grammar));
    let plan = plan_with(sql, Some(grammar), Some(settings_for_grammar));
    let Some(mut range) = replacement_range(sql, cursor, &plan) else {
        return Candidates {
            range: cursor..cursor,
            values: Vec::new(),
        };
    };
    let quoted = sql[range.start..].starts_with(['"', '`', '[']);
    let shared_plan = index.grammar.map(|_| &plan);
    let scope = scope_at(sql, range.start, index, settings, shared_plan);
    if scope == Scope::Tables {
        range.start = qualified_start(sql, range.start, &plan);
    }
    let prefix = normalized(&sql[range.start..cursor]);
    let mut values = candidate_sql(sql, range.start, index, settings, Some(&prefix), shared_plan);
    if !quoted && !matches!(scope, Scope::ColumnsOf(_)) {
        values.extend(super::completion::SQL_KEYWORDS.split_whitespace().map(str::to_string));
    }
    let values = values
        .into_iter()
        .filter(|value| {
            normalized(value).starts_with(&prefix)
                || split_table_reference(value, index.grammar)
                    .is_some_and(|(_, name)| normalized(&name).starts_with(&prefix))
        })
        .map(|value| {
            let label = split_table_reference(&value, index.grammar)
                .filter(|(schema, _)| schema.is_none() && value.starts_with(['"', '`', '[']))
                .map(|(_, name)| name)
                .unwrap_or_else(|| value.clone());
            (label, value)
        })
        .collect();
    Candidates { range, values }
}

fn replacement_range(sql: &str, cursor: usize, plan: &ScriptPlan) -> Option<Range<usize>> {
    let token = plan
        .tokens(0..sql.len())
        .find(|token| {
            token.range.start == cursor
                && matches!(
                    token.kind,
                    ScriptTokenKind::Word | ScriptTokenKind::QuotedIdentifier | ScriptTokenKind::Unterminated
                )
        })
        .or_else(|| {
            plan.tokens(0..sql.len())
                .find(|token| token.range.start < cursor && token.range.end >= cursor)
        });
    let Some(token) = token else {
        return Some(cursor..cursor);
    };
    match token.kind {
        ScriptTokenKind::Word | ScriptTokenKind::QuotedIdentifier => Some(token.range),
        ScriptTokenKind::Unterminated if token.range.start == cursor => None,
        ScriptTokenKind::Unterminated
            if plan.diagnostics().iter().any(|diagnostic| {
                matches!(
                    diagnostic,
                    ScriptDiagnostic::Unterminated { construct: OpenConstruct::QuotedIdentifier, start }
                        if *start == token.range.start
                )
            }) =>
        {
            Some(token.range.start..cursor)
        }
        ScriptTokenKind::Whitespace
        | ScriptTokenKind::Punct
        | ScriptTokenKind::OpenParen
        | ScriptTokenKind::CloseParen
        | ScriptTokenKind::Semicolon => Some(cursor..cursor),
        _ => None,
    }
}

fn qualified_start(sql: &str, start: usize, plan: &ScriptPlan) -> usize {
    let tokens: Vec<_> = plan.tokens(0..start).filter(|token| !token.kind.is_trivia()).collect();
    let mut start = start;
    for pair in tokens.rchunks_exact(2) {
        let [name, dot] = pair else {
            break;
        };
        if &sql[dot.range.clone()] != "."
            || !matches!(name.kind, ScriptTokenKind::Word | ScriptTokenKind::QuotedIdentifier)
        {
            break;
        }
        start = name.range.start;
    }
    start
}

#[cfg(test)]
#[path = "completion_proposals_tests.rs"]
mod tests;
