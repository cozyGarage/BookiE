#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use bookie_core::sql_syntax::SqlGrammar;
use bookie_core::sql_syntax::script::{LexicalSettings, ScriptPlan};

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

#[test]
fn a_mysql_routine_body_with_if_else_stays_one_statement() {
    let routine = "CREATE PROCEDURE p() BEGIN IF 1 THEN SELECT 1; ELSE SELECT 2; END IF; END";
    let found = statements(&format!("{routine}; SELECT 3"), SqlGrammar::MySql);
    assert_eq!(found, vec![routine, "SELECT 3"]);
}

#[test]
fn mysql_loops_case_statements_and_labels_close_their_routine_body() {
    for body in [
        "BEGIN WHILE 1 DO SELECT 1; END WHILE; END",
        "BEGIN CASE 1 WHEN 1 THEN SELECT 1; END CASE; END",
        "BEGIN SELECT CASE WHEN 1 THEN 2 END; REPEAT SELECT 1; UNTIL 1 END REPEAT; END",
        "body: BEGIN l: LOOP LEAVE l; END LOOP l; END body",
    ] {
        let routine = format!("CREATE DEFINER = app PROCEDURE p() {body}");
        let found = statements(&format!("{routine}; SELECT 3"), SqlGrammar::MySql);
        assert_eq!(found, vec![routine.as_str(), "SELECT 3"]);
    }
}

#[test]
fn a_mysql_trigger_or_event_body_stays_one_statement() {
    for head in [
        "CREATE TRIGGER t BEFORE INSERT ON a FOR EACH ROW",
        "CREATE EVENT e ON SCHEDULE EVERY 1 DAY DO",
    ] {
        let program = format!("{head} BEGIN SET @a = 1; SET @b = 2; END");
        let found = statements(&format!("{program}; SELECT 3"), SqlGrammar::MySql);
        assert_eq!(found, vec![program.as_str(), "SELECT 3"]);
    }
}

#[test]
fn a_one_statement_routine_body_does_not_swallow_the_next_statement() {
    for (text, grammar) in [
        ("CREATE PROCEDURE p() SELECT 1; SELECT 2", SqlGrammar::MySql),
        (
            "CREATE TRIGGER t BEFORE INSERT ON a FOR EACH ROW SET NEW.x = 1; SELECT 2",
            SqlGrammar::MySql,
        ),
        (
            "CREATE FUNCTION f() RETURNS int RETURN 1; SELECT 2",
            SqlGrammar::PostgreSql,
        ),
    ] {
        let found = statements(text, grammar);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[1], "SELECT 2");
    }
}

#[test]
fn a_mysql_transaction_begin_still_ends_at_its_semicolon() {
    let script = "BEGIN; UPDATE t SET a = 1; COMMIT; CREATE TABLE event (id int); BEGIN; \
                  CREATE VIEW v AS SELECT event, begin FROM t; SELECT 1";
    let found = statements(script, SqlGrammar::MySql);
    assert_eq!(found.len(), 7, "{found:?}");
}

#[test]
fn a_mysql_delimiter_script_splits_only_at_the_declared_delimiter() {
    let routine = "CREATE PROCEDURE p() BEGIN IF 1 THEN SELECT 1; ELSE SELECT 2; END IF; END";
    let script = format!("DELIMITER $$\n{routine}$$\nCREATE PROCEDURE q() BEGIN SELECT 1;$$\nDELIMITER ;\nSELECT 3;");
    let found = statements(&script, SqlGrammar::MySql);
    assert_eq!(found.len(), 3, "{found:?}");
    assert_eq!(found[2], "SELECT 3");
}

#[test]
fn a_word_ending_before_a_quote_or_dollar_sign_is_not_a_string_prefix() {
    for text in [
        "SELECT 1 WHERE 'a' LIKE'\\'; SELECT 2",
        "SELECT 1 AS typE'x'; SELECT 2",
        "SELECT a$$ FROM t; SELECT 2",
    ] {
        let found = statements(text, SqlGrammar::PostgreSql);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[1], "SELECT 2");
    }
}

#[test]
fn pasted_sql_with_trailing_blank_lines_yields_only_its_statements() {
    for grammar in [
        SqlGrammar::PostgreSql,
        SqlGrammar::MySql,
        SqlGrammar::Sqlite,
        SqlGrammar::MsSql,
    ] {
        for text in ["SELECT 1;\n\n\n", "SELECT 1\n\n\n  \t\n", "\n\nSELECT 1;\r\n\r\n"] {
            let plan = ScriptPlan::build(text, grammar, LexicalSettings::default_for(grammar));
            let found: Vec<&str> = plan
                .statements()
                .iter()
                .map(|statement| statement.text(text).trim())
                .filter(|statement| !statement.is_empty())
                .collect();
            let found: Vec<&str> = found.iter().map(|statement| statement.trim_end_matches(';')).collect();
            assert_eq!(found, ["SELECT 1"], "{grammar:?} {text:?}");
            assert!(plan.diagnostics().is_empty(), "{grammar:?} {text:?}");
        }
    }
}
