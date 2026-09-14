//! Successful-statement catalog notifications. This does not authorize execution.
use sqlparser::{ast::Statement, parser::Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogEffect {
    None,
    Ddl,
    Begin,
    Commit { chain: bool },
    Rollback { chain: bool },
}

pub fn catalog_effect(sql: &str, driver_id: &str) -> CatalogEffect {
    let dialect = super::classify::dialect_for(driver_id);
    let Ok(statements) = Parser::parse_sql(dialect.as_ref(), sql) else {
        return CatalogEffect::None;
    };
    match statements.as_slice() {
        [Statement::StartTransaction { .. }] => CatalogEffect::Begin,
        [Statement::Commit { chain, .. }] => CatalogEffect::Commit { chain: *chain },
        [Statement::Rollback { chain, savepoint: None }] => CatalogEffect::Rollback { chain: *chain },
        [_] if super::classify(sql, driver_id).contains_ddl => CatalogEffect::Ddl,
        _ => CatalogEffect::None,
    }
}
