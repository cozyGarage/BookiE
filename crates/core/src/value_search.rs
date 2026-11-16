use crate::filter::{
    FilterValueKind, escape_like, filter_column, filter_placeholder, filter_value_kind, has_native_ilike,
    like_needs_escape_clause, parse_filter_value, pattern_column,
};
use crate::query::{ColumnInfo, Value};
use crate::sql_dialect::placeholder_for;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Contains,
    Exact,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchClause {
    pub sql: String,
    pub value: Value,
}

pub fn search_clause(
    driver_id: &str,
    column: &ColumnInfo,
    term: &str,
    mode: SearchMode,
    include_non_text: bool,
    placeholder_index: usize,
) -> Option<SearchClause> {
    let kind = filter_value_kind(column);
    match (mode, kind) {
        (SearchMode::Contains, FilterValueKind::Text) => {
            Some(contains_clause(driver_id, column, term, placeholder_index))
        }
        (SearchMode::Exact, FilterValueKind::Text) => {
            let value = parse_filter_value(driver_id, column, term).ok()?;
            Some(equals_clause(driver_id, column, value, placeholder_index))
        }
        (SearchMode::Exact, FilterValueKind::Int | FilterValueKind::Decimal | FilterValueKind::Uuid)
            if include_non_text =>
        {
            let value = parse_filter_value(driver_id, column, term).ok()?;
            Some(equals_clause(driver_id, column, value, placeholder_index))
        }
        _ => None,
    }
}

fn contains_clause(driver_id: &str, column: &ColumnInfo, term: &str, placeholder_index: usize) -> SearchClause {
    let operator = if has_native_ilike(driver_id) { "ILIKE" } else { "LIKE" };
    let escape = match driver_id {
        "mysql" => " ESCAPE '|'",
        _ if like_needs_escape_clause(driver_id) => " ESCAPE '\\'",
        _ => "",
    };
    SearchClause {
        sql: format!(
            "{} {operator} {}{escape}",
            pattern_column(driver_id, column),
            placeholder_for(driver_id, placeholder_index)
        ),
        value: Value::Text(format!("%{}%", escape_like(driver_id, term))),
    }
}

fn equals_clause(driver_id: &str, column: &ColumnInfo, value: Value, placeholder_index: usize) -> SearchClause {
    SearchClause {
        sql: format!(
            "{} = {}",
            filter_column(driver_id, column),
            filter_placeholder(driver_id, column, placeholder_index)
        ),
        value,
    }
}

#[cfg(test)]
#[path = "value_search_tests.rs"]
mod tests;
