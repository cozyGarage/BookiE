use bookie_core::{Combinator, FilterOp, FilterRule, FilterSet, FilterValue, SearchMode};

use super::TableOutcome;

pub fn open_filter(outcome: &TableOutcome, term: &str, mode: SearchMode) -> FilterSet {
    let (op, value) = match mode {
        SearchMode::Exact => (FilterOp::Eq, term.to_owned()),
        SearchMode::Contains => (FilterOp::IContains, term.to_owned()),
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
