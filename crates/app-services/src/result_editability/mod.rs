mod source_table;
mod target;

pub use source_table::{SourceItem, SourceName, SourceRefusal, SourceTable, source_table};
pub use target::{
    EditTarget, ResultLock, ResultRefusal, can_stage_result_value, editable_target, mutation_columns,
    owned_result_columns,
};

use bookie_core::sql_syntax::SqlGrammar;

pub fn grammar_for_driver(driver_id: &str) -> Option<SqlGrammar> {
    match driver_id {
        "postgres" | "duckdb" => Some(SqlGrammar::PostgreSql),
        "mysql" => Some(SqlGrammar::MySql),
        "sqlite" => Some(SqlGrammar::Sqlite),
        "mssql" => Some(SqlGrammar::MsSql),
        "clickhouse" => Some(SqlGrammar::ClickHouse),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
