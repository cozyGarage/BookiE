use bookie_core::sql_dialect::{IdentError, build_order_and_pagination, quote_ident, validate_ident};
use bookie_core::{ColumnInfo, FilterValueKind, Value, filter_value_kind, search_clause};
use thiserror::Error;

use super::{
    MAX_DISPLAY_COLUMNS, MAX_HITS_PER_TABLE, MAX_SEARCH_COLUMNS_PER_TABLE, MAX_TERM_CHARS, SearchOptions, SearchTable,
    TablePlan,
};

#[derive(Debug, PartialEq, Eq, Error)]
pub enum PlanError {
    #[error("type the value to search for")]
    EmptyTerm,

    #[error("the search value can be at most {max} characters")]
    TermTooLong { max: usize },

    #[error("{0}")]
    Ident(#[from] IdentError),
}

pub fn validate_term(term: &str) -> Result<&str, PlanError> {
    if term.trim().is_empty() {
        return Err(PlanError::EmptyTerm);
    }
    if term.chars().count() > MAX_TERM_CHARS {
        return Err(PlanError::TermTooLong { max: MAX_TERM_CHARS });
    }
    Ok(term)
}

pub fn plan_table(
    driver_id: &str,
    table: &SearchTable,
    term: &str,
    options: SearchOptions,
) -> Result<Option<TablePlan>, PlanError> {
    let term = validate_term(term)?;
    validate_ident(&table.target.name)?;
    if let Some(schema) = &table.target.schema {
        validate_ident(schema)?;
    }
    let mut candidates = Vec::new();
    let mut unsearched = 0usize;
    for column in &table.columns {
        validate_ident(&column.name)?;
        let probe = search_clause(driver_id, column, term, options.mode, options.include_non_text, 0);
        match probe {
            Some(_) if candidates.len() < MAX_SEARCH_COLUMNS_PER_TABLE => candidates.push(column),
            Some(_) => unsearched += 1,
            None => {}
        }
    }
    if candidates.is_empty() {
        return Ok(None);
    }
    let display = display_columns(&table.columns);
    Ok(Some(build_plan(
        driver_id,
        table,
        term,
        options,
        &candidates,
        &display,
        unsearched,
    )))
}

fn display_columns(columns: &[ColumnInfo]) -> Vec<&ColumnInfo> {
    let mut chosen: Vec<&ColumnInfo> = columns
        .iter()
        .filter(|column| column.primary_key)
        .take(MAX_DISPLAY_COLUMNS)
        .collect();
    if chosen.is_empty() {
        chosen = columns
            .iter()
            .filter(|column| {
                !matches!(
                    filter_value_kind(column),
                    FilterValueKind::Bytes | FilterValueKind::Json
                )
            })
            .take(MAX_DISPLAY_COLUMNS)
            .collect();
    }
    chosen
}

fn build_plan(
    driver_id: &str,
    table: &SearchTable,
    term: &str,
    options: SearchOptions,
    candidates: &[&ColumnInfo],
    display: &[&ColumnInfo],
    unsearched: usize,
) -> TablePlan {
    let mut params: Vec<Value> = Vec::with_capacity(candidates.len() * 2);
    let mut flags = Vec::with_capacity(candidates.len());
    let mut conditions = Vec::with_capacity(candidates.len());
    for column in candidates {
        let flag = search_clause(
            driver_id,
            column,
            term,
            options.mode,
            options.include_non_text,
            params.len(),
        );
        if let Some(clause) = flag {
            flags.push(format!(
                "CASE WHEN {} THEN 1 ELSE 0 END AS {}",
                clause.sql,
                quote_ident(driver_id, &format!("__bookie_hit_{}", flags.len()))
            ));
            params.push(clause.value);
        }
    }
    for column in candidates {
        let condition = search_clause(
            driver_id,
            column,
            term,
            options.mode,
            options.include_non_text,
            params.len(),
        );
        if let Some(clause) = condition {
            conditions.push(clause.sql);
            params.push(clause.value);
        }
    }
    let shown: Vec<String> = display
        .iter()
        .map(|column| quote_ident(driver_id, &column.name))
        .collect();
    let select: Vec<String> = shown.into_iter().chain(flags).collect();
    let statement = format!(
        "SELECT {} FROM {} WHERE {}{}",
        select.join(", "),
        qualified_table(driver_id, table),
        conditions.join(" OR "),
        build_order_and_pagination(driver_id, None, MAX_HITS_PER_TABLE + 1, 0)
    );
    TablePlan {
        target: table.target.clone(),
        statement,
        params,
        searched: candidates.iter().map(|column| column.name.clone()).collect(),
        display: display.iter().map(|column| column.name.clone()).collect(),
        columns_not_searched: unsearched,
    }
}

fn qualified_table(driver_id: &str, table: &SearchTable) -> String {
    let name = quote_ident(driver_id, &table.target.name);
    match &table.target.schema {
        Some(schema) => format!("{}.{name}", quote_ident(driver_id, schema)),
        None => name,
    }
}
