pub(crate) fn read_only_statement(sql: &str) -> Option<String> {
    let body = strip_leading_comments(sql);
    let body = body.trim_end().trim_end_matches(';').trim_end();
    let first = body.split(|c: char| !c.is_ascii_alphanumeric()).next()?;
    if !first.eq_ignore_ascii_case("select") && !first.eq_ignore_ascii_case("with") {
        return None;
    }
    let upper = body.to_ascii_uppercase();
    let locks = upper
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair == ["FOR", "UPDATE"]);
    (!locks).then(|| body.to_string())
}

fn strip_leading_comments(sql: &str) -> &str {
    let mut rest = sql.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("--") {
            rest = after.split_once('\n').map_or("", |(_, tail)| tail).trim_start();
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, tail)| tail).trim_start();
        } else {
            return rest;
        }
    }
}

pub(crate) fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub(crate) fn qualified(schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(schema) => format!("{}.{}", quote_identifier(schema), quote_identifier(table)),
        None => quote_identifier(table),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_and_with_clauses_pass_and_lose_the_trailing_semicolon() {
        assert_eq!(
            read_only_statement("select 1 from dual;").as_deref(),
            Some("select 1 from dual")
        );
        assert_eq!(
            read_only_statement("  WITH t AS (SELECT 1 a FROM dual) SELECT a FROM t").as_deref(),
            Some("WITH t AS (SELECT 1 a FROM dual) SELECT a FROM t")
        );
    }

    #[test]
    fn comments_before_the_keyword_do_not_hide_it() {
        assert_eq!(
            read_only_statement(
                "-- note
/* more */ select 1 from dual"
            )
            .as_deref(),
            Some("select 1 from dual")
        );
        assert_eq!(read_only_statement("/* select */ delete from t"), None);
    }

    #[test]
    fn writes_ddl_plsql_and_locking_reads_are_refused() {
        for sql in [
            "insert into t values (1)",
            "update t set a = 1",
            "delete from t",
            "drop table t",
            "begin null; end;",
            "alter session set current_schema = x",
            "select * from t for  update",
            "select * from t FOR UPDATE NOWAIT",
            "",
            "   ",
        ] {
            assert_eq!(read_only_statement(sql), None, "{sql}");
        }
    }

    #[test]
    fn a_column_called_for_update_text_in_a_string_is_still_refused_conservatively() {
        assert_eq!(read_only_statement("select 'x' from dual for update"), None);
    }

    #[test]
    fn identifiers_are_quoted_and_embedded_quotes_are_doubled() {
        assert_eq!(quote_identifier("A\"B"), "\"A\"\"B\"");
        assert_eq!(qualified(Some("HR"), "EMP"), "\"HR\".\"EMP\"");
        assert_eq!(qualified(None, "EMP"), "\"EMP\"");
    }
}
