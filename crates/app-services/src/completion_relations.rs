use bookie_core::sql_syntax::SqlGrammar;
use bookie_core::sql_syntax::script::{LexicalSettings, ScriptToken, ScriptTokenKind};

use super::completion::{SchemaIndex, table_references};
use super::completion_identifiers::{normalized, plan_with, quoted_name};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OutputColumn {
    Named(String),
    Star(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Relation {
    pub name: String,
    pub display: String,
    pub is_cte: bool,
    pub columns: Vec<OutputColumn>,
    pub body: std::ops::Range<usize>,
}

struct Scan<'a> {
    text: &'a str,
    grammar: Option<SqlGrammar>,
    tokens: Vec<ScriptToken>,
}

pub(super) fn relations(text: &str, grammar: Option<SqlGrammar>, settings: Option<LexicalSettings>) -> Vec<Relation> {
    let plan = plan_with(text, grammar, settings);
    let tokens = plan
        .tokens(0..text.len())
        .filter(|token| !token.kind.is_trivia())
        .collect();
    let scan = Scan { text, grammar, tokens };
    let mut found = scan.ctes();
    found.extend(scan.derived_tables());
    found
}

pub(super) struct Lookup<'a> {
    pub relations: &'a [Relation],
    pub text: &'a str,
    pub index: &'a SchemaIndex,
    pub settings: Option<LexicalSettings>,
}

impl Lookup<'_> {
    pub(super) fn columns(&self, name: &str) -> Option<Vec<String>> {
        let named = |target: &str| {
            self.relations
                .iter()
                .find(|relation| normalized(&relation.name) == normalized(target))
        };
        if let Some(relation) = named(name) {
            return Some(self.expand(relation));
        }
        let references = table_references(self.text, self.index.grammar, self.settings);
        let table = references
            .iter()
            .find(|(_, alias)| {
                alias
                    .as_deref()
                    .is_some_and(|alias| normalized(alias) == normalized(name))
            })
            .map(|(table, _)| table.as_str())?;
        named(table).map(|relation| self.expand(relation))
    }

    pub(super) fn columns_after(&self, qualifier: Option<&str>, resolved: &str) -> Option<Vec<String>> {
        qualifier
            .and_then(|qualifier| self.columns(qualifier))
            .or_else(|| self.columns(resolved))
    }

    pub(super) fn local_candidates(&self, column_sql: &impl Fn(&str) -> String) -> Vec<String> {
        let names = self
            .relations
            .iter()
            .filter(|relation| relation.is_cte)
            .map(|relation| relation.display.clone());
        let derived = self
            .relations
            .iter()
            .filter(|relation| !relation.is_cte)
            .flat_map(|relation| self.expand(relation))
            .map(|column| column_sql(&column));
        names.chain(derived).collect()
    }

    fn expand(&self, relation: &Relation) -> Vec<String> {
        let body = self.text.get(relation.body.clone()).unwrap_or_default();
        let references = table_references(body, self.index.grammar, self.settings);
        let table_columns = |table: &str| self.index.columns_for(table).cloned().unwrap_or_default();
        let mut out = Vec::new();
        for column in &relation.columns {
            match column {
                OutputColumn::Named(name) => out.push(name.clone()),
                OutputColumn::Star(None) => {
                    for (table, _) in &references {
                        out.extend(table_columns(table));
                    }
                }
                OutputColumn::Star(Some(qualifier)) => {
                    let table = references
                        .iter()
                        .find(|(_, alias)| {
                            alias
                                .as_deref()
                                .is_some_and(|alias| normalized(alias) == normalized(qualifier))
                        })
                        .map_or(qualifier.as_str(), |(table, _)| table.as_str());
                    out.extend(table_columns(table));
                }
            }
        }
        out
    }
}

impl Scan<'_> {
    fn raw(&self, token: &ScriptToken) -> &str {
        &self.text[token.range.clone()]
    }

    fn is_word(&self, index: usize, word: &str) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|token| token.kind == ScriptTokenKind::Word && self.raw(token).eq_ignore_ascii_case(word))
    }

    fn is_punct(&self, index: usize, symbol: &str) -> bool {
        self.tokens.get(index).is_some_and(|token| self.raw(token) == symbol)
    }

    fn identifier(&self, index: usize) -> Option<String> {
        let token = self.tokens.get(index)?;
        match token.kind {
            ScriptTokenKind::Word => Some(self.raw(token).to_string()),
            ScriptTokenKind::QuotedIdentifier => quoted_name(self.raw(token), self.grammar),
            _ => None,
        }
    }

    fn group_end(&self, open: usize) -> usize {
        let mut depth = 0usize;
        for (offset, token) in self.tokens[open..].iter().enumerate() {
            match token.kind {
                ScriptTokenKind::OpenParen => depth += 1,
                ScriptTokenKind::CloseParen => depth = depth.saturating_sub(1),
                _ => continue,
            }
            if depth == 0 {
                return open + offset;
            }
        }
        self.tokens.len()
    }

    fn display(&self, index: usize) -> String {
        self.tokens
            .get(index)
            .map_or_else(String::new, |token| self.raw(token).to_string())
    }

    fn span(&self, from: usize, to: usize) -> std::ops::Range<usize> {
        let end = self.tokens.get(to).map_or(self.text.len(), |token| token.range.start);
        let start = self.tokens.get(from).map_or(end, |token| token.range.start).min(end);
        start..end
    }

    fn ctes(&self) -> Vec<Relation> {
        let mut found = Vec::new();
        for start in 0..self.tokens.len() {
            if !self.is_word(start, "with") {
                continue;
            }
            let mut index = start + 1 + usize::from(self.is_word(start + 1, "recursive"));
            while let Some(name) = self.identifier(index) {
                let index_of_name = index;
                let (explicit, after_name) = self.explicit_columns(index + 1);
                if !self.is_word(after_name, "as") {
                    break;
                }
                index = after_name + 1;
                while self.is_word(index, "not") || self.is_word(index, "materialized") {
                    index += 1;
                }
                if self.tokens.get(index).map(|token| token.kind) != Some(ScriptTokenKind::OpenParen) {
                    break;
                }
                let close = self.group_end(index);
                let columns = explicit.unwrap_or_else(|| self.output_columns(index + 1, close));
                found.push(Relation {
                    name,
                    display: self.display(index_of_name),
                    is_cte: true,
                    columns,
                    body: self.span(index + 1, close),
                });
                index = close + 1;
                if !self.is_punct(index, ",") {
                    break;
                }
                index += 1;
            }
        }
        found
    }

    fn explicit_columns(&self, index: usize) -> (Option<Vec<OutputColumn>>, usize) {
        if self.tokens.get(index).map(|token| token.kind) != Some(ScriptTokenKind::OpenParen) {
            return (None, index);
        }
        let close = self.group_end(index);
        let names = (index + 1..close)
            .filter(|at| !self.is_punct(*at, ","))
            .filter_map(|at| self.identifier(at).map(OutputColumn::Named))
            .collect();
        (Some(names), close + 1)
    }

    fn derived_tables(&self) -> Vec<Relation> {
        let mut found = Vec::new();
        for open in 1..self.tokens.len() {
            let after_from =
                self.is_word(open - 1, "from") || self.is_word(open - 1, "join") || self.is_punct(open - 1, ",");
            let is_subquery = self.tokens[open].kind == ScriptTokenKind::OpenParen
                && (self.is_word(open + 1, "select") || self.is_word(open + 1, "with"));
            if !after_from || !is_subquery {
                continue;
            }
            let close = self.group_end(open);
            let alias_at = close + 1 + usize::from(self.is_word(close + 1, "as"));
            let Some(name) = self.identifier(alias_at).filter(|name| !is_clause_word(name)) else {
                continue;
            };
            found.push(Relation {
                name,
                display: self.display(alias_at),
                is_cte: false,
                columns: self.output_columns(open + 1, close),
                body: self.span(open + 1, close),
            });
        }
        found
    }

    fn output_columns(&self, from: usize, to: usize) -> Vec<OutputColumn> {
        let Some(select) = (from..to).find(|at| self.is_word(*at, "select")) else {
            return Vec::new();
        };
        let mut index = select + 1;
        while self.is_word(index, "distinct") || self.is_word(index, "all") {
            index += 1;
        }
        if self.is_word(index, "top") {
            index += 2;
        }
        let mut columns = Vec::new();
        let mut item_start = index;
        let mut depth = 0usize;
        while index < to {
            match self.tokens[index].kind {
                ScriptTokenKind::OpenParen => depth += 1,
                ScriptTokenKind::CloseParen => depth = depth.saturating_sub(1),
                _ => {}
            }
            let ends_list = depth == 0 && self.is_word(index, "from");
            if ends_list || (depth == 0 && self.is_punct(index, ",")) {
                columns.extend(self.item_column(item_start, index));
                item_start = index + 1;
            }
            if ends_list {
                return columns;
            }
            index += 1;
        }
        columns.extend(self.item_column(item_start, to));
        columns
    }

    fn item_column(&self, from: usize, to: usize) -> Option<OutputColumn> {
        let length = to.checked_sub(from)?;
        if length == 0 {
            return None;
        }
        if length >= 2 && self.is_word(to - 2, "as") {
            return self.identifier(to - 1).map(OutputColumn::Named);
        }
        if length == 1 && self.is_punct(from, "*") {
            return Some(OutputColumn::Star(None));
        }
        if length == 3 && self.is_punct(from + 1, ".") && self.is_punct(from + 2, "*") {
            return Some(OutputColumn::Star(self.identifier(from)));
        }
        if self.is_dotted_identifier(from, to) {
            return self.identifier(to - 1).map(OutputColumn::Named);
        }
        let name = self.identifier(to - 1)?;
        let alias = length >= 2 && !is_clause_word(&name) && self.ends_an_operand(to - 2);
        alias.then_some(OutputColumn::Named(name))
    }

    fn is_dotted_identifier(&self, from: usize, to: usize) -> bool {
        (from..to).all(|at| {
            if (at - from).is_multiple_of(2) {
                self.identifier(at).is_some()
            } else {
                self.is_punct(at, ".")
            }
        }) && (to - from) % 2 == 1
    }

    fn ends_an_operand(&self, index: usize) -> bool {
        self.tokens.get(index).is_some_and(|token| match token.kind {
            ScriptTokenKind::CloseParen | ScriptTokenKind::Number | ScriptTokenKind::StringLiteral => true,
            ScriptTokenKind::Word => !is_clause_word(self.raw(token)),
            ScriptTokenKind::QuotedIdentifier => true,
            _ => false,
        })
    }
}

fn is_clause_word(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "end"
            | "on"
            | "using"
            | "where"
            | "group"
            | "order"
            | "limit"
            | "union"
            | "join"
            | "left"
            | "right"
            | "inner"
            | "full"
            | "cross"
            | "outer"
            | "natural"
            | "set"
            | "having"
            | "window"
            | "select"
            | "from"
            | "and"
            | "or"
            | "not"
            | "is"
            | "in"
            | "like"
            | "between"
            | "when"
            | "then"
            | "else"
            | "case"
            | "as"
    )
}
