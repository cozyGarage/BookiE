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

const NON_SOURCE_OWNERS: [&str; 6] = ["extract", "trim", "substring", "substr", "position", "overlay"];

const NON_SOURCE_BEFORE: [&str; 4] = ["distinct", "both", "leading", "trailing"];

const SAFE_TABLE_FUNCTIONS: [&str; 7] = [
    "range",
    "generate_series",
    "generate_subscripts",
    "unnest",
    "json_each",
    "json_tree",
    "repeat_row",
];

#[derive(Default)]
struct Scan {
    owners: Vec<String>,
    from_depths: Vec<usize>,
    source_expected: bool,
}

pub(crate) fn reads_a_path(tokens: &[Token]) -> bool {
    let significant: Vec<&Token> = tokens
        .iter()
        .filter(|token| !matches!(token, Token::Whitespace(_)))
        .collect();
    let mut scan = Scan::default();
    for (index, token) in significant.iter().enumerate() {
        let previous = index.checked_sub(1).and_then(|before| significant.get(before));
        if scan.source_expected && is_path_or_foreign_source(token, significant.get(index + 1)) {
            return true;
        }
        scan.advance(token, previous.copied(), significant.get(index + 1).copied());
    }
    false
}

impl Scan {
    fn depth(&self) -> usize {
        self.owners.len()
    }

    fn advance(&mut self, token: &Token, previous: Option<&Token>, next: Option<&Token>) {
        match token {
            Token::LParen => {
                let owner = word_text(previous).unwrap_or_default();
                self.owners.push(owner);
            }
            Token::RParen => {
                self.owners.pop();
                let depth = self.depth();
                self.from_depths.retain(|active| *active <= depth);
                self.source_expected = false;
            }
            Token::Comma => self.source_expected = self.from_depths.last() == Some(&self.depth()),
            Token::Period => {}
            Token::Word(word) if word.quote_style.is_none() => self.keyword(&word.value, previous, next),
            _ => self.source_expected = false,
        }
    }

    fn keyword(&mut self, text: &str, previous: Option<&Token>, next: Option<&Token>) {
        let lowered = text.to_ascii_lowercase();
        if lowered == "from" || lowered == "join" {
            if self.is_source_keyword(previous) {
                let depth = self.depth();
                if self.from_depths.last() != Some(&depth) {
                    self.from_depths.push(depth);
                }
                self.source_expected = true;
            } else {
                self.source_expected = false;
            }
            return;
        }
        if lowered == "describe" || lowered == "summarize" {
            self.source_expected = true;
            return;
        }
        if lowered == "lateral" || matches!(next, Some(Token::Period)) {
            return;
        }
        if CLAUSE_ENDS.contains(&lowered.as_str()) {
            let depth = self.depth();
            self.from_depths.retain(|active| *active != depth);
        }
        self.source_expected = false;
    }

    fn is_source_keyword(&self, previous: Option<&Token>) -> bool {
        let after_special = word_text(previous).is_some_and(|word| NON_SOURCE_BEFORE.contains(&word.as_str()));
        let inside_special = self
            .owners
            .last()
            .is_some_and(|owner| NON_SOURCE_OWNERS.contains(&owner.as_str()));
        !after_special && !inside_special
    }
}

fn word_text(token: Option<&Token>) -> Option<String> {
    match token {
        Some(Token::Word(word)) if word.quote_style.is_none() => Some(word.value.to_ascii_lowercase()),
        _ => None,
    }
}

fn is_path_or_foreign_source(token: &Token, next: Option<&&Token>) -> bool {
    match token {
        Token::SingleQuotedString(_) => true,
        Token::Word(word) if word.quote_style.is_some() => looks_like_a_path(&word.value),
        Token::Word(word) => {
            matches!(next, Some(Token::LParen)) && !is_safe_table_function(&word.value.to_ascii_lowercase())
        }
        _ => false,
    }
}

fn is_safe_table_function(name: &str) -> bool {
    SAFE_TABLE_FUNCTIONS.contains(&name) || name.starts_with("duckdb_") || name.starts_with("pragma_")
}

fn looks_like_a_path(name: &str) -> bool {
    name.contains('/') || name.contains('\\') || name.contains('.')
}
