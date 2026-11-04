use crate::query::ForeignKeyInfo;
use crate::sql_dialect::quote_ident;

pub const MAX_JOINS: usize = 20;
const PREVIEW_LIMIT: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JoinSkeletonError {
    #[error("a join skeleton is available for PostgreSQL and MySQL/MariaDB only")]
    Unsupported(String),
    #[error("this table declares no foreign keys, or the engine does not report them")]
    NoForeignKeys,
    #[error("the foreign key {0} is incomplete, so no join was built")]
    InvalidKey(String),
    #[error("this table has more foreign keys than a skeleton can join")]
    TooManyForeignKeys,
}

pub fn join_skeleton(
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    foreign_keys: &[ForeignKeyInfo],
) -> Result<String, JoinSkeletonError> {
    if !matches!(driver_id, "postgres" | "mysql") {
        return Err(JoinSkeletonError::Unsupported(driver_id.to_string()));
    }
    if foreign_keys.is_empty() {
        return Err(JoinSkeletonError::NoForeignKeys);
    }
    if foreign_keys.len() > MAX_JOINS {
        return Err(JoinSkeletonError::TooManyForeignKeys);
    }
    let mut joins = Vec::with_capacity(foreign_keys.len());
    for (position, key) in foreign_keys.iter().enumerate() {
        joins.push(join_clause(driver_id, key, position + 1)?);
    }
    let selected = (0..=foreign_keys.len())
        .map(|alias| format!("t{alias}.*"))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "SELECT {selected}\nFROM {} AS t0\n{}\nLIMIT {PREVIEW_LIMIT};",
        qualified(driver_id, schema, table),
        joins.join("\n")
    ))
}

fn join_clause(driver_id: &str, key: &ForeignKeyInfo, alias: usize) -> Result<String, JoinSkeletonError> {
    let complete = !key.columns.is_empty()
        && key.columns.len() == key.ref_columns.len()
        && !key.ref_table.trim().is_empty()
        && key
            .columns
            .iter()
            .chain(&key.ref_columns)
            .all(|name| !name.trim().is_empty());
    if !complete {
        return Err(JoinSkeletonError::InvalidKey(key.name.clone()));
    }
    let conditions = key
        .columns
        .iter()
        .zip(&key.ref_columns)
        .map(|(from, to)| {
            format!(
                "t0.{} = t{alias}.{}",
                quote_ident(driver_id, from),
                quote_ident(driver_id, to)
            )
        })
        .collect::<Vec<_>>()
        .join(" AND ");
    Ok(format!(
        "LEFT JOIN {} AS t{alias} ON {conditions}",
        qualified(driver_id, key.ref_schema.as_deref(), &key.ref_table)
    ))
}

fn qualified(driver_id: &str, schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(schema) => format!("{}.{}", quote_ident(driver_id, schema), quote_ident(driver_id, table)),
        None => quote_ident(driver_id, table),
    }
}

#[cfg(test)]
#[path = "join_skeleton_tests.rs"]
mod tests;
