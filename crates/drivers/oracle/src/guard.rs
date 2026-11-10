pub(crate) fn read_only_statement(sql: &str) -> Option<String> {
    let body = strip_leading_comments(sql);
    let body = body.trim_end().trim_end_matches(';').trim_end();
    let first = body.split(|c: char| !c.is_ascii_alphanumeric()).next()?;
    if !first.eq_ignore_ascii_case("select") && !first.eq_ignore_ascii_case("with") {
        return None;
    }
    let words = code_words(body);
    let locks = words.windows(2).any(|pair| pair == ["FOR", "UPDATE"]);
    (!locks && !words.iter().any(|word| word == UNTERMINATED)).then(|| body.to_string())
}

const UNTERMINATED: &str = "<UNTERMINATED>";

struct Scanner {
    chars: Vec<char>,
    at: usize,
    words: Vec<String>,
}

impl Scanner {
    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).copied()
    }

    fn skip_until(&mut self, ends: impl Fn(&Self) -> bool, width: usize) {
        while self.at < self.chars.len() && !ends(self) {
            self.at += 1;
        }
        if self.at >= self.chars.len() {
            self.words.push(UNTERMINATED.into());
        }
        self.at += width;
    }

    fn line_comment(&mut self) {
        while self.at < self.chars.len() && self.chars[self.at] != '\n' {
            self.at += 1;
        }
    }

    fn block_comment(&mut self) {
        self.at += 2;
        self.skip_until(|scan| scan.peek(0) == Some('*') && scan.peek(1) == Some('/'), 2);
    }

    fn alternative_quote(&mut self) {
        let closing = match self.peek(2).unwrap_or('\'') {
            '[' => ']',
            '(' => ')',
            '{' => '}',
            '<' => '>',
            other => other,
        };
        self.at += 3;
        self.skip_until(|scan| scan.peek(0) == Some(closing) && scan.peek(1) == Some('\''), 2);
        self.words.push("'".into());
    }

    fn quoted(&mut self, quote: char) {
        self.at += 1;
        while self.at < self.chars.len() {
            if self.chars[self.at] == quote {
                if self.peek(1) == Some(quote) {
                    self.at += 2;
                    continue;
                }
                break;
            }
            self.at += 1;
        }
        if self.at >= self.chars.len() {
            self.words.push(UNTERMINATED.into());
        }
        self.at += 1;
        self.words.push(quote.to_string());
    }
}

fn code_words(sql: &str) -> Vec<String> {
    let mut scan = Scanner {
        chars: sql.chars().collect(),
        at: 0,
        words: Vec::new(),
    };
    let mut word = String::new();
    while let Some(current) = scan.peek(0) {
        let next = scan.peek(1);
        let skipped = (current == '-' && next == Some('-'))
            || (current == '/' && next == Some('*'))
            || (matches!(current, 'q' | 'Q') && next == Some('\'') && word.is_empty())
            || current == '\''
            || current == '"';
        if !skipped && (current.is_alphanumeric() || matches!(current, '_' | '$' | '#')) {
            word.push(current);
            scan.at += 1;
            continue;
        }
        if !word.is_empty() {
            scan.words.push(std::mem::take(&mut word).to_ascii_uppercase());
        }
        match (current, next) {
            ('-', Some('-')) => scan.line_comment(),
            ('/', Some('*')) => scan.block_comment(),
            ('q' | 'Q', Some('\'')) => scan.alternative_quote(),
            ('\'' | '"', _) => scan.quoted(current),
            _ => scan.at += 1,
        }
    }
    if !word.is_empty() {
        scan.words.push(word.to_ascii_uppercase());
    }
    scan.words
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
    fn a_real_locking_clause_is_refused_after_a_string_literal() {
        assert_eq!(read_only_statement("select 'x' from dual for update"), None);
    }

    #[test]
    fn comments_between_for_and_update_do_not_hide_the_locking_clause() {
        for sql in [
            "select * from t for/* c */update",
            "select * from t for/**/update",
            "select * from t for -- note\n update",
            "select * from t FOR /* a */ /* b */ UPDATE NOWAIT",
            "select * from t for\n--x\n/*y*/\tupdate of a",
            "select * from t for/* unterminated update",
            "select 'unterminated for update from t",
            "select q'[unterminated from t",
            "with c as (select 1 a from dual) select * from c for/*x*/update",
        ] {
            assert_eq!(
                read_only_statement(sql),
                Some(sql.trim().to_string()).filter(|_| false),
                "{sql}"
            );
        }
    }

    #[test]
    fn quoted_text_and_unrelated_comments_are_not_a_locking_clause() {
        for sql in [
            "select 'for update' from dual",
            "select \"for update\" from t",
            "select 'it''s for update' from t",
            "select q'[for update]' from dual",
            "select a /* for update */ from t",
            "select a from t -- for update",
            "select a as \"for\", b as \"update\" from t",
            "select forupdate, update_count from t",
        ] {
            assert_eq!(read_only_statement(sql).as_deref(), Some(sql.trim_end()), "{sql}");
        }
    }

    #[test]
    fn identifiers_are_quoted_and_embedded_quotes_are_doubled() {
        assert_eq!(quote_identifier("A\"B"), "\"A\"\"B\"");
        assert_eq!(qualified(Some("HR"), "EMP"), "\"HR\".\"EMP\"");
        assert_eq!(qualified(None, "EMP"), "\"EMP\"");
    }
}
