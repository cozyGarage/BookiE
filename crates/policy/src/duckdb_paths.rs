use sqlparser::tokenizer::Token;

const CLAUSE_ENDS: [&str; 11] = [
    "where",
    "group",
    "order",
    "having",
    "limit",
    "union",
    "intersect",
    "except",
    "select",
    "window",
    "qualify",
];

const FILE_SUFFIXES: [&str; 9] = [
    ".csv", ".tsv", ".parquet", ".json", ".ndjson", ".jsonl", ".xlsx", ".db", ".duckdb",
];

pub(crate) fn reads_a_path(tokens: &[Token]) -> bool {
    let mut depth = 0_usize;
    let mut from_depth = None;
    let mut source_expected = false;
    for token in tokens.iter().filter(|token| !matches!(token, Token::Whitespace(_))) {
        match token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            Token::Comma if from_depth == Some(depth) => {
                source_expected = true;
                continue;
            }
            Token::Word(word) if word.quote_style.is_none() => {
                let lowered = word.value.to_ascii_lowercase();
                if lowered == "from" || lowered == "join" {
                    from_depth = Some(depth);
                    source_expected = true;
                    continue;
                }
                if CLAUSE_ENDS.contains(&lowered.as_str()) && from_depth == Some(depth) {
                    from_depth = None;
                }
            }
            _ => {}
        }
        if source_expected && is_path_source(token) {
            return true;
        }
        source_expected = false;
    }
    false
}

fn is_path_source(token: &Token) -> bool {
    match token {
        Token::SingleQuotedString(_) => true,
        Token::Word(word) if word.quote_style == Some('"') => looks_like_a_path(&word.value),
        _ => false,
    }
}

fn looks_like_a_path(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    lowered.contains('/') || lowered.contains('\\') || FILE_SUFFIXES.iter().any(|suffix| lowered.ends_with(suffix))
}
