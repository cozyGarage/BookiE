use sqlx::{MySql, Pool, Row};
use tablepro_core::{DriverError, TableInfo};

pub(super) async fn list_relations(pool: &Pool<MySql>, kind: Option<&str>) -> Result<Vec<TableInfo>, DriverError> {
    let rows = sqlx::query(
        "SELECT CAST(table_schema AS CHAR), CAST(table_name AS CHAR)
         FROM information_schema.tables
         WHERE table_schema = DATABASE() AND (? IS NULL OR table_type = ?)
         ORDER BY table_name",
    )
    .bind(kind)
    .bind(kind)
    .fetch_all(pool)
    .await
    .map_err(super::map_sqlx_error)?;
    Ok(rows
        .into_iter()
        .map(|row| TableInfo {
            schema: Some(row.get::<String, _>(0)),
            name: row.get::<String, _>(1),
        })
        .collect())
}
