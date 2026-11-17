use std::collections::HashSet;

use bookie_core::{DriverError, ViewEditability};
use sqlx::{AssertSqlSafe, Column, Executor, MySqlConnection, Row, SqlSafeStr, Statement};

use crate::map_sqlx_error;

pub(super) async fn fetch(
    connection: &mut MySqlConnection,
    schema: Option<&str>,
    view: &str,
) -> Result<ViewEditability, DriverError> {
    let Some((schema, updatable)) = sqlx::query(
        "SELECT CAST(table_schema AS CHAR), is_updatable
         FROM information_schema.views
         WHERE table_schema = COALESCE(?, DATABASE()) AND table_name = ?",
    )
    .bind(schema)
    .bind(view)
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_sqlx_error)?
    .map(|row| (row.get::<String, _>(0), row.get::<String, _>(1) == "YES")) else {
        return Ok(ViewEditability::NotApplicable);
    };
    if !updatable {
        return Ok(ViewEditability::NotUpdatable);
    }

    let sql = format!(
        "SELECT * FROM `{}`.`{}` LIMIT 0",
        schema.replace('`', "``"),
        view.replace('`', "``")
    );
    let description = connection
        .prepare_with(AssertSqlSafe(sql.as_str()).into_sql_str(), &[])
        .await
        .map_err(map_sqlx_error)?;
    let origins = description
        .columns()
        .iter()
        .filter_map(|column| {
            column.origin().table_column().map(|origin| {
                (
                    column.name().to_owned(),
                    origin.table.to_string(),
                    origin.name.to_string(),
                )
            })
        })
        .collect::<Vec<_>>();
    let base_tables = origins.iter().map(|(_, table, _)| table).collect::<HashSet<_>>();
    if base_tables.len() != 1 {
        return Ok(ViewEditability::NotUpdatable);
    }
    let Some(base_table) = base_tables.iter().next().copied() else {
        return Ok(ViewEditability::NotUpdatable);
    };
    let Some((base_schema, base_name)) = base_table.rsplit_once('.') else {
        return Ok(ViewEditability::NotUpdatable);
    };
    let primary_key = sqlx::query(
        "SELECT CAST(column_name AS CHAR)
         FROM information_schema.key_column_usage
         WHERE table_schema = ? AND table_name = ? AND constraint_name = 'PRIMARY'
         ORDER BY ordinal_position",
    )
    .bind(base_schema)
    .bind(base_name)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?
    .into_iter()
    .map(|row| row.get::<String, _>(0))
    .collect::<Vec<_>>();
    if primary_key.is_empty() {
        return Ok(ViewEditability::KeyNotExposed);
    }
    let Some(key_columns) = primary_key
        .iter()
        .map(|key| {
            origins
                .iter()
                .find(|(_, table, column)| table == base_table && column == key)
                .map(|(view_column, _, _)| view_column.clone())
        })
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(ViewEditability::KeyNotExposed);
    };

    Ok(ViewEditability::Editable {
        key_columns,
        read_only_columns: description
            .columns()
            .iter()
            .filter(|column| column.origin().table_column().is_none())
            .map(|column| column.name().to_owned())
            .collect(),
        insertable: false,
    })
}
