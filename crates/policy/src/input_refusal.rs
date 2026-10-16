use sqlparser::dialect::Dialect;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

pub(crate) fn refusal(sql: &str, driver_id: &str, dialect: &dyn Dialect) -> Option<&'static str> {
    match driver_id {
        "redis" => Some("Redis commands are not SQL and are not classified as reads"),
        "mysql" if contains_mysql_executable_comment(sql, dialect) => {
            Some("MySQL executable comments are not safely classified")
        }
        _ => None,
    }
}

fn contains_mysql_executable_comment(sql: &str, dialect: &dyn Dialect) -> bool {
    let Ok(tokens) = Tokenizer::new(dialect, sql).tokenize() else {
        return false;
    };

    tokens.iter().any(|token| {
        matches!(token, Token::Whitespace(Whitespace::MultiLineComment(comment))
            if comment.starts_with('!') || comment.starts_with("M!"))
    })
}
