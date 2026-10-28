use std::ops::Range;

use tablepro_core::sql_syntax::SqlGrammar;
use tablepro_core::sql_syntax::script::{
    LexicalSettings, OpenConstruct, ScriptDiagnostic, ScriptPlan, ScriptTokenKind,
};

use super::completion::{SchemaIndex, Scope, candidate_sql, scope_at};
use super::completion_identifiers::{normalized, split_table_reference};

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
    let plan = ScriptPlan::build(sql, grammar, settings_for_grammar);
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
mod tests {
    use super::*;

    #[test]
    fn table_completion_replaces_a_qualified_reference_as_a_whole() {
        let mut index = SchemaIndex::default();
        index.set_tables(vec!["\"odd.schema\".\"Users\"".into()]);
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
    fn completion_after_a_backslash_ending_string_follows_the_session_string_rules() {
        let mut index = SchemaIndex::default();
        index.grammar = Some(SqlGrammar::MySql);
        index.set_columns("orders", vec!["total".into()]);
        let sql = r"SELECT 'a\', o. FROM orders o";
        let cursor = r"SELECT 'a\', o.".len();
        assert!(candidates(sql, cursor, &index).values.is_empty());
        let no_backslash_escapes = Some(LexicalSettings::from_mysql_sql_mode("NO_BACKSLASH_ESCAPES"));
        let values = candidates_with(sql, cursor, &index, no_backslash_escapes).values;
        assert_eq!(values, vec![("total".into(), "`total`".into())]);
        assert_eq!(
            super::super::completion::referenced_tables_with(sql, cursor, index.grammar, no_backslash_escapes),
            vec!["orders"]
        );
    }

    #[test]
    fn whitespace_after_an_alias_dot_keeps_only_its_columns() {
        let mut index = SchemaIndex::default();
        index.grammar = Some(SqlGrammar::PostgreSql);
        index.set_columns("users", vec!["user_column".into()]);
        index.set_columns("orders", vec!["order_column".into()]);
        let sql = "SELECT u.  FROM users u JOIN orders o ON true";
        let values = candidates(sql, "SELECT u. ".len(), &index).values;
        assert_eq!(values, vec![("user_column".into(), "\"user_column\"".into())]);
    }

    #[test]
    fn unfinished_identifier_quotes_keep_the_exact_reference_for_each_dialect() {
        for (grammar, driver) in [
            (SqlGrammar::PostgreSql, "postgres"),
            (SqlGrammar::MySql, "mysql"),
            (SqlGrammar::MsSql, "mssql"),
            (SqlGrammar::Sqlite, "sqlite"),
            (SqlGrammar::ClickHouse, "clickhouse"),
        ] {
            let mut index = SchemaIndex::default();
            index.grammar = Some(grammar);
            let reference = tablepro_core::sql_dialect::quote_ident(driver, "Users");
            index.set_tables(vec![reference.clone()]);
            let partial = format!("{}Us", &reference[..1]);
            let sql = format!("SELECT * FROM {partial}");
            let candidates = candidates(&sql, sql.len(), &index);
            assert_eq!(candidates.values, vec![("Users".into(), reference)]);
            assert_eq!(&sql[candidates.range], partial);
        }
    }

    #[test]
    fn a_cursor_at_the_start_of_an_identifier_replaces_its_whole_token() {
        let sql = "SELECT * FROM \"Users\" WHERE true";
        let cursor = "SELECT * FROM ".len();
        let candidates = candidates(sql, cursor, &SchemaIndex::default());
        assert_eq!(&sql[candidates.range], "\"Users\"");
    }

    #[test]
    fn completion_before_an_unfinished_quote_refuses_an_ambiguous_suffix() {
        let mut index = SchemaIndex::default();
        index.set_tables(vec!["\"Users\"".into()]);
        let sql = "SELECT * FROM \"Us WHERE true";
        assert!(candidates(sql, "SELECT * FROM ".len(), &index).values.is_empty());
    }

    #[test]
    fn mysql_unterminated_double_quotes_remain_a_string_literal() {
        let mut index = SchemaIndex::default();
        index.grammar = Some(SqlGrammar::MySql);
        let sql = "SELECT \"SE";
        assert!(candidates(sql, sql.len(), &index).values.is_empty());
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

    fn reference_candidates(sql: &str, cursor: usize, index: &SchemaIndex) -> Candidates {
        let grammar = index.grammar.unwrap_or(SqlGrammar::PostgreSql);
        let settings = LexicalSettings::default_for(grammar);
        let Some(mut range) = replacement_range(sql, cursor, &ScriptPlan::build(sql, grammar, settings)) else {
            return Candidates {
                range: cursor..cursor,
                values: Vec::new(),
            };
        };
        let quoted = sql[range.start..].starts_with(['"', '`', '[']);
        let scope = scope_at(sql, range.start, index, None, None);
        if scope == Scope::Tables {
            range.start = qualified_start(sql, range.start, &ScriptPlan::build(sql, grammar, settings));
        }
        let mut values = candidate_sql(sql, range.start, index, None, None, None);
        if !quoted && !matches!(scope, Scope::ColumnsOf(_)) {
            values.extend(
                super::super::completion::SQL_KEYWORDS
                    .split_whitespace()
                    .map(str::to_string),
            );
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

    #[test]
    fn the_indexed_catalog_gives_the_same_candidates_as_filtering_every_name() {
        let tables: Vec<String> = [
            "users",
            "Users",
            "orders",
            "public.users",
            "public.Orders",
            "public.order_items",
            "audit.user_log",
            "\"odd.schema\".\"Users\"",
            "\"Mixed\"",
            "\"a\"\"b\"",
            "`shop`.`items`",
            "`shop`.`Orders`",
            "[dbo].[Orders]",
            "[dbo].[users]",
            "[Weird Name]",
            "",
            "schema.",
            ".name",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let queries = [
            "SELECT * FROM ",
            "SELECT * FROM us",
            "SELECT * FROM Us",
            "SELECT * FROM public.o",
            "SELECT * FROM \"odd.schema\".\"Us",
            "SELECT * FROM \"Mi",
            "SELECT * FROM `shop`.`i",
            "SELECT * FROM [dbo].[O",
            "SELECT u. FROM users u",
            "SELECT us FROM users u WHERE u.id = 1",
            "SELECT 1; SELECT * FROM public.o; SELECT 2",
            "UPDATE o",
            "INSERT INTO a",
            "SELECT * FROM users JOIN or",
            "-- comment\nSELECT * FROM au",
            "SELECT 'it''s; FROM us' FROM us",
            "SELECT $$ FROM x; $$, 1 FROM us",
            "SEL",
            "",
        ];
        let mut compared = 0;
        for grammar in [
            Some(SqlGrammar::PostgreSql),
            Some(SqlGrammar::MySql),
            Some(SqlGrammar::MsSql),
            Some(SqlGrammar::Sqlite),
            None,
        ] {
            let mut index = SchemaIndex::default();
            index.grammar = grammar;
            index.set_tables(tables.clone());
            for sql in queries {
                for cursor in (0..=sql.len()).filter(|cursor| sql.is_char_boundary(*cursor)) {
                    let fast = candidates(sql, cursor, &index);
                    let slow = reference_candidates(sql, cursor, &index);
                    assert_eq!(fast.range, slow.range, "range for {sql:?} at {cursor} ({grammar:?})");
                    assert_eq!(fast.values, slow.values, "values for {sql:?} at {cursor} ({grammar:?})");
                    compared += 1;
                }
            }
        }
        assert!(compared > 1000, "the comparison covered {compared} cases");
    }

    #[test]
    fn a_catalog_replaced_after_the_keys_were_built_is_matched_against_its_new_tables() {
        let sql = "SELECT * FROM al";
        let offered = |index: &SchemaIndex| -> Vec<String> {
            candidates(sql, sql.len(), index)
                .values
                .into_iter()
                .map(|(_, value)| value)
                .collect()
        };
        let mut index = SchemaIndex::default();
        index.grammar = Some(SqlGrammar::PostgreSql);
        index.set_tables(vec!["alpha".into(), "beta".into()]);
        assert!(offered(&index).contains(&"alpha".to_string()));
        index.set_tables(vec!["alpine".into(), "alps".into(), "beta".into()]);
        let after = offered(&index);
        assert!(after.contains(&"alpine".to_string()) && after.contains(&"alps".to_string()));
        assert!(!after.contains(&"alpha".to_string()) && !after.contains(&"beta".to_string()));
        index.grammar = Some(SqlGrammar::MySql);
        assert!(offered(&index).contains(&"alpine".to_string()));
        index.clear();
        assert!(!offered(&index).contains(&"alpine".to_string()));
    }
}
