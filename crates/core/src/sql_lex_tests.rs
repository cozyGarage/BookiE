use super::{skip_span, split_statements, statement_at_cursor};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn skip_span_returns_exact_byte_lengths_for_representative_spans() {
    let cases = [
        ("-- note\n;", "postgres", 8),
        ("/*x*/;", "postgres", 5),
        ("# note\n;", "mysql", 7),
        ("'a\\';b';", "mysql", 7),
        ("E'a\\';b';", "postgres", 8),
        ("`a\\`;b`;", "clickhouse", 7),
        ("[a]]b];", "mssql", 6),
        ("$x$;body$x$;", "postgres", 11),
    ];

    for (sql, driver, expected) in cases {
        assert_eq!(skip_span(sql, driver), Some(expected), "{driver}: {sql}");
    }
    assert_eq!(skip_span("$1$body$1$;", "postgres"), None);
    assert_eq!(skip_span("$a-b$body$a-b$;", "postgres"), None);
}

#[test]
fn mysql_block_comments_close_at_the_first_terminator() {
    assert_eq!(
        split_statements("SELECT 1 /* outer /* inner */ ; SELECT 2", "mysql"),
        vec!["SELECT 1 /* outer /* inner */", "SELECT 2"]
    );
}

#[test]
fn empty_sql_and_cursor_at_end_are_safe() {
    assert!(split_statements("", "postgres").is_empty());
    assert_eq!(statement_at_cursor("SELECT 1", "postgres", 8), Some("SELECT 1".into()));
}

#[test]
fn escaped_quotes_and_nested_comments_keep_statement_boundaries() {
    let cases = [
        ("mysql", "SELECT 'a\\';b'"),
        ("clickhouse", "SELECT 'a\\';b'"),
        ("postgres", "SELECT E'a\\';b'"),
        ("postgres", "SELECT 1 /* outer /* x */ ; still outer */"),
        ("mssql", "SELECT [a]];b]"),
        ("mysql", "SELECT `a``;b`"),
    ];
    for (driver, first) in cases {
        let sql = format!("{first}; SELECT 2");
        assert_eq!(split_statements(&sql, driver), vec![first, "SELECT 2"], "{driver}");
        assert_eq!(
            statement_at_cursor(&sql, driver, first.find(';').unwrap()),
            Some(first.into())
        );
    }
    assert_eq!(
        split_statements("SELECT '\\'; SELECT 2", "postgres"),
        vec!["SELECT '\\'", "SELECT 2"]
    );
}

#[test]
fn a_postgres_dollar_quoted_body_stays_one_statement() {
    let sql = "CREATE FUNCTION f() RETURNS int AS $$ BEGIN RETURN 1; END; $$ LANGUAGE plpgsql";
    assert_eq!(split_statements(sql, "postgres"), vec![sql]);
}

#[test]
fn a_tagged_dollar_quoted_body_stays_one_statement_next_to_another() {
    let sql = "CREATE FUNCTION f() AS $body$ SELECT 1; SELECT 2; $body$ LANGUAGE sql; SELECT 3";
    let split = split_statements(sql, "postgres");
    assert_eq!(split.len(), 2);
    assert!(split[0].contains("SELECT 1; SELECT 2;"));
    assert_eq!(split[1], "SELECT 3");
}

#[test]
fn a_dollar_sign_inside_an_identifier_does_not_open_a_dollar_quote() {
    assert_eq!(
        split_statements("SELECT my$a$b FROM t; SELECT 2", "postgres"),
        ["SELECT my$a$b FROM t", "SELECT 2"]
    );
    assert_eq!(
        split_statements("SELECT $a$ x; y $a$; SELECT 2", "postgres"),
        ["SELECT $a$ x; y $a$", "SELECT 2"]
    );
}

#[test]
fn a_mysql_dollar_delimiter_is_not_a_quoted_span() {
    assert_eq!(
        split_statements("SELECT $tag$; SELECT 2", "mysql"),
        vec!["SELECT $tag$", "SELECT 2"]
    );
}

#[test]
fn scanner_progress_makes_bounded_inputs_finish() {
    const CHILD: &str = "BOOKIEE_SQL_LEX_PROGRESS_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let cases = [
            (
                "/* outer /* inner */ still outer */; SELECT 1",
                "postgres",
                vec!["/* outer /* inner */ still outer */", "SELECT 1"],
            ),
            (
                "SELECT 1 /* outer /* x */ ; still outer */; SELECT 2",
                "postgres",
                vec!["SELECT 1 /* outer /* x */ ; still outer */", "SELECT 2"],
            ),
            ("SELECT 'a;b'; SELECT 2", "postgres", vec!["SELECT 'a;b'", "SELECT 2"]),
            ("SELECT [a;b]; SELECT 2", "mssql", vec!["SELECT [a;b]", "SELECT 2"]),
            (
                "SELECT 1 /* a;b */; SELECT 2",
                "postgres",
                vec!["SELECT 1 /* a;b */", "SELECT 2"],
            ),
            ("SELECT 1; SELECT 2", "postgres", vec!["SELECT 1", "SELECT 2"]),
            ("SELECT '東京'; SELECT 2", "postgres", vec!["SELECT '東京'", "SELECT 2"]),
            (
                "CREATE FUNCTION f() AS $body$ SELECT 1; SELECT 2; $body$ LANGUAGE sql; SELECT 3",
                "postgres",
                vec![
                    "CREATE FUNCTION f() AS $body$ SELECT 1; SELECT 2; $body$ LANGUAGE sql",
                    "SELECT 3",
                ],
            ),
            ("SELECT $tag$; SELECT 2", "mysql", vec!["SELECT $tag$", "SELECT 2"]),
        ];
        for (sql, driver, expected) in cases {
            assert_eq!(split_statements(sql, driver), expected, "{driver}: {sql}");
        }
        return;
    }

    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "sql_lex::tests::scanner_progress_makes_bounded_inputs_finish",
        ])
        .env(CHILD, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "scanner child failed: {status}");
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("scanner did not finish bounded inputs within two seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_mysql_double_dash_needs_whitespace_to_start_a_comment() {
    assert_eq!(
        split_statements("SELECT 5--2; SELECT 1", "mysql"),
        ["SELECT 5--2", "SELECT 1"]
    );
    assert_eq!(skip_span("--2; SELECT 1", "mysql"), None);
    assert_eq!(skip_span("-- note\nSELECT 1", "mysql"), Some(8));
    assert_eq!(skip_span("--\tnote", "mysql"), Some(7));
    assert_eq!(skip_span("--", "mysql"), Some(2));
    assert_eq!(
        split_statements("SELECT 5--2; SELECT 1", "postgres"),
        ["SELECT 5--2; SELECT 1"]
    );
}

#[test]
fn a_mysql_hash_comment_does_not_end_a_statement() {
    let sql = "SELECT 1 # note ; here\n; SELECT 2";
    assert_eq!(
        split_statements(sql, "mysql"),
        vec!["SELECT 1 # note ; here", "SELECT 2"]
    );
}

#[test]
fn a_hash_is_not_a_comment_for_a_dialect_without_one() {
    assert_eq!(split_statements("SELECT 1 # a ; b", "postgres").len(), 2);
}

#[test]
fn a_backtick_identifier_holding_a_semicolon_does_not_split() {
    let sql = "SELECT `col;name` FROM t; SELECT 2";
    assert_eq!(split_statements(sql, "mysql").len(), 2);
    assert_eq!(split_statements(sql, "clickhouse").len(), 2);
}

#[test]
fn a_bracket_identifier_holding_a_semicolon_does_not_split() {
    let sql = "SELECT [col;name] FROM t; SELECT 2";
    assert_eq!(split_statements(sql, "mssql").len(), 2);
}

#[test]
fn the_cursor_inside_a_dollar_quoted_body_selects_the_whole_statement() {
    let sql = "CREATE FUNCTION f() AS $$ SELECT 1; SELECT 2; $$ LANGUAGE sql; SELECT 3";
    let picked = statement_at_cursor(sql, "postgres", 30).unwrap();
    assert!(picked.starts_with("CREATE FUNCTION"));
    assert!(picked.ends_with("LANGUAGE sql"));
}

#[test]
fn an_unterminated_span_keeps_the_tail_as_one_statement() {
    assert_eq!(
        split_statements("SELECT 1; SELECT 'open; SELECT 2", "postgres"),
        vec!["SELECT 1", "SELECT 'open; SELECT 2"]
    );
    assert_eq!(
        split_statements("SELECT 1; /* open ; SELECT 2", "postgres"),
        vec!["SELECT 1", "/* open ; SELECT 2"]
    );
}
