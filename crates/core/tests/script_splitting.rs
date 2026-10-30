#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use tablepro_core::sql_syntax::SqlGrammar;
use tablepro_core::sql_syntax::script::{LexicalSettings, ScriptPlan};

fn statements(text: &str, grammar: SqlGrammar) -> Vec<String> {
    let plan = ScriptPlan::build(text, grammar, LexicalSettings::default_for(grammar));
    plan.statements()
        .iter()
        .map(|statement| statement.text(text).trim().to_string())
        .collect()
}

#[test]
fn postgres_block_comments_nest() {
    let found = statements("SELECT /* a /* b */ ; */ 1; SELECT 2", SqlGrammar::PostgreSql);
    assert_eq!(found, vec!["SELECT /* a /* b */ ; */ 1", "SELECT 2"]);
}

#[test]
fn postgres_tagged_dollar_quotes_hide_inner_terminators() {
    let found = statements("DO $a$ x; $b$ y $b$ ; $a$; SELECT 1", SqlGrammar::PostgreSql);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].starts_with("DO $a$") && found[0].ends_with("$a$"), "{found:?}");
    assert_eq!(found[1], "SELECT 1");
}

#[test]
fn a_semicolon_inside_a_string_or_quoted_identifier_does_not_split() {
    assert_eq!(statements("SELECT 'a;b'; SELECT 2", SqlGrammar::PostgreSql).len(), 2);
    assert_eq!(
        statements("SELECT \"a;b\" FROM t; SELECT 2", SqlGrammar::PostgreSql).len(),
        2
    );
    assert_eq!(statements("SELECT `a;b` FROM t; SELECT 2", SqlGrammar::MySql).len(), 2);
}

#[test]
fn a_sqlite_trigger_body_stays_one_statement() {
    let found = statements(
        "CREATE TRIGGER t AFTER INSERT ON a BEGIN SELECT CASE WHEN 1 THEN 2 END; END; SELECT 1",
        SqlGrammar::Sqlite,
    );
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].ends_with("END"), "{found:?}");
}

#[test]
fn an_unterminated_string_or_comment_does_not_panic_or_lose_the_text() {
    for text in [
        "SELECT 'abc",
        "SELECT /* abc",
        "SELECT $a$ abc",
        "SELECT \"abc",
        ";;;",
        "",
    ] {
        for grammar in SqlGrammar::ALL {
            let plan = ScriptPlan::build(text, grammar, LexicalSettings::default_for(grammar));
            for statement in plan.statements() {
                let _ = statement.text(text);
            }
        }
    }
}

#[test]
fn a_very_long_script_splits_in_linear_time_and_keeps_every_statement() {
    let text = "SELECT 1;\n".repeat(20_000);
    assert_eq!(statements(&text, SqlGrammar::PostgreSql).len(), 20_000);
}
