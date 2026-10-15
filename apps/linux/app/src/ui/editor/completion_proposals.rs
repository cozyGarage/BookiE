use std::ops::Range;

use tablepro_core::sql_syntax::SqlGrammar;
use tablepro_core::sql_syntax::script::{LexicalSettings, ScriptPlan, ScriptTokenKind};

use super::completion::{SchemaIndex, Scope, candidate_sql, scope_at};
use super::completion_identifiers::split_table_reference;

pub(super) struct Candidates {
    pub range: Range<usize>,
    pub values: Vec<(String, String)>,
}

pub(super) fn candidates(sql: &str, cursor: usize, index: &SchemaIndex) -> Candidates {
    let grammar = index.grammar.unwrap_or(SqlGrammar::PostgreSql);
    let Some(mut range) = replacement_range(sql, cursor, grammar) else {
        return Candidates {
            range: cursor..cursor,
            values: Vec::new(),
        };
    };
    let scope = scope_at(sql, range.start, index);
    if scope == Scope::Tables {
        range.start = qualified_start(sql, range.start, grammar);
    }
    let mut values = candidate_sql(sql, range.start, index);
    if !matches!(scope, Scope::ColumnsOf(_)) {
        values.extend(super::SQL_KEYWORDS.split_whitespace().map(str::to_string));
    }
    let prefix = normalized(&sql[range.start..cursor]);
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

fn normalized(text: &str) -> String {
    text.chars()
        .filter(|character| !matches!(character, '\"' | '`' | '[' | ']'))
        .collect::<String>()
        .to_lowercase()
}

fn replacement_range(sql: &str, cursor: usize, grammar: SqlGrammar) -> Option<Range<usize>> {
    let plan = ScriptPlan::build(sql, grammar, LexicalSettings::default_for(grammar));
    let token = plan
        .tokens(0..sql.len())
        .find(|token| token.range.start < cursor && token.range.end >= cursor);
    let Some(token) = token else {
        return Some(cursor..cursor);
    };
    match token.kind {
        ScriptTokenKind::Word | ScriptTokenKind::QuotedIdentifier => Some(token.range),
        ScriptTokenKind::Unterminated if identifier_quote(&sql[token.range.start..], grammar) => {
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

fn identifier_quote(text: &str, grammar: SqlGrammar) -> bool {
    text.starts_with('"')
        || (matches!(grammar, SqlGrammar::MySql | SqlGrammar::ClickHouse | SqlGrammar::Sqlite) && text.starts_with('`'))
        || (matches!(grammar, SqlGrammar::MsSql | SqlGrammar::Sqlite) && text.starts_with('['))
}

fn qualified_start(sql: &str, start: usize, grammar: SqlGrammar) -> usize {
    let plan = ScriptPlan::build(sql, grammar, LexicalSettings::default_for(grammar));
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
mod tests {
    use super::*;

    #[test]
    fn table_completion_replaces_a_qualified_reference_as_a_whole() {
        let mut index = SchemaIndex::default();
        index.tables = vec!["\"odd.schema\".\"Users\"".into()];
        index.grammar = Some(SqlGrammar::PostgreSql);
        let sql = "SELECT * FROM \"odd.schema\".\"Us\" WHERE true";
        let cursor = sql.find("Us").unwrap() + 1;
        let candidates = candidates(sql, cursor, &index);
        assert_eq!(&sql[candidates.range], "\"odd.schema\".\"Us\"");
        assert_eq!(
            candidates.values,
            vec![("\"odd.schema\".\"Users\"".into(), "\"odd.schema\".\"Users\"".into())]
        );
    }

    #[test]
    fn columns_use_each_engine_delimiters_and_never_offer_unrelated_editor_words() {
        for (grammar, driver) in [
            (SqlGrammar::PostgreSql, "postgres"),
            (SqlGrammar::MySql, "mysql"),
            (SqlGrammar::MsSql, "mssql"),
            (SqlGrammar::Sqlite, "sqlite"),
            (SqlGrammar::ClickHouse, "clickhouse"),
        ] {
            let mut index = SchemaIndex::default();
            index.grammar = Some(grammar);
            index.set_columns("users", vec!["a.b\"` ]".into()]);
            let sql = "SELECT u. FROM users u; SELECT unrelated_editor_word";
            let values = candidates(sql, "SELECT u.".len(), &index).values;
            assert_eq!(values.len(), 1);
            assert_eq!(values[0].1, tablepro_core::sql_dialect::quote_ident(driver, "a.b\"` ]"));
        }
    }

    #[test]
    fn literals_and_comments_do_not_offer_sql_identifiers() {
        let index = SchemaIndex::default();
        for sql in [
            "SELECT 'word",
            "SELECT 'word'",
            "SELECT -- word",
            "SELECT /* word",
            "SELECT $$word",
            "SELECT 123",
        ] {
            assert!(candidates(sql, sql.len(), &index).values.is_empty(), "{sql}");
        }
    }
}
