use bookie_core::{DriverError, TableInfo};
use sqlx::{Pool, Postgres, Row};

use crate::map_sqlx_error;

pub(crate) async fn list_relations(
    pool: &Pool<Postgres>,
    relation_kinds: &[&str],
    excluded_schemas: &[String],
) -> Result<Vec<TableInfo>, DriverError> {
    let relation_kinds = relation_kinds
        .iter()
        .map(|kind| (*kind).to_string())
        .collect::<Vec<_>>();
    let rows = sqlx::query(
        "SELECT n.nspname, c.relname
         FROM pg_catalog.pg_class c
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
         WHERE c.relkind::text = ANY($1)
           AND n.nspname NOT IN ('pg_catalog', 'information_schema')
           AND NOT EXISTS (
             SELECT 1 FROM pg_catalog.unnest($2::text[]) AS hidden_schema(schema_name)
             WHERE hidden_schema.schema_name = n.nspname
           )
           AND pg_catalog.has_schema_privilege(n.oid, 'USAGE')
           AND pg_catalog.has_any_column_privilege(c.oid, 'SELECT,INSERT,UPDATE,REFERENCES')
         ORDER BY n.nspname, c.relname",
    )
    .bind(relation_kinds)
    .bind(excluded_schemas)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx_error)?;
    Ok(rows
        .into_iter()
        .map(|row| TableInfo {
            schema: Some(row.get::<String, _>(0)),
            name: row.get::<String, _>(1),
        })
        .collect())
}
