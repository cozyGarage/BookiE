use bookie_core::{Combinator, FilterOp, FilterRule, FilterSet, FilterValue, SearchMode};

use super::TableOutcome;

const WILDCARD_CHARACTERS: [char; 4] = ['%', '_', '\\', '['];

pub fn open_filter(outcome: &TableOutcome, term: &str, mode: SearchMode) -> FilterSet {
    let (op, value) = match mode {
        SearchMode::Exact => (FilterOp::Eq, term.to_owned()),
        SearchMode::Contains if term.contains(WILDCARD_CHARACTERS) => (FilterOp::Contains, term.to_owned()),
        SearchMode::Contains => (FilterOp::Ilike, format!("%{term}%")),
    };
    let rules = outcome
        .matched_columns
        .iter()
        .map(|column| FilterRule {
            column: column.clone(),
            op,
            value: Some(FilterValue::Single(value.clone())),
        })
        .collect();
    FilterSet {
        combinator: Combinator::Or,
        rules,
        extra_sql: None,
    }
}
