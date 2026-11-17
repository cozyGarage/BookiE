use bookie_core::{DriverError, ViewEditability};
use sqlx::{MySqlConnection, Row};

use crate::{map_sqlx_error, view_definition};

pub(super) async fn fetch(
    connection: &mut MySqlConnection,
    schema: Option<&str>,
    view: &str,
) -> Result<ViewEditability, DriverError> {
    let Some(info) = view_info(connection, schema, view).await? else {
        return Ok(ViewEditability::NotApplicable);
    };
    if !info.updatable {
        return Ok(ViewEditability::NotUpdatable);
    }
    let Some(parsed) = view_definition::parse(&info.definition) else {
        return Ok(ViewEditability::NotUpdatable);
    };
    if parsed.base_schema.as_deref().is_some_and(|base| base != info.schema) {
        return Ok(ViewEditability::NotUpdatable);
    }
    let primary_key = primary_key(connection, &info.schema, &parsed.base_table).await?;
    if primary_key.is_empty() {
        return Ok(ViewEditability::KeyNotExposed);
    }
    let Some(key_columns) = exposed_keys(&parsed, &primary_key) else {
        return Ok(ViewEditability::KeyNotExposed);
    };
    let read_only_columns = parsed
        .columns
        .iter()
        .filter(|column| column.origin.is_none())
        .map(|column| column.alias.clone())
        .collect();
    Ok(ViewEditability::Editable {
        key_columns,
        read_only_columns,
        insertable: false,
    })
}

struct ViewInfo {
    schema: String,
    updatable: bool,
    definition: String,
}

async fn view_info(
    connection: &mut MySqlConnection,
    schema: Option<&str>,
    view: &str,
) -> Result<Option<ViewInfo>, DriverError> {
    Ok(sqlx::query(
        "SELECT CAST(table_schema AS CHAR), is_updatable, CAST(view_definition AS CHAR)
         FROM information_schema.views
         WHERE table_schema = COALESCE(?, DATABASE()) AND table_name = ?",
    )
    .bind(schema)
    .bind(view)
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_sqlx_error)?
    .map(|row| ViewInfo {
        schema: row.get::<String, _>(0),
        updatable: row.get::<String, _>(1) == "YES",
        definition: row.get::<String, _>(2),
    }))
}

async fn primary_key(connection: &mut MySqlConnection, schema: &str, table: &str) -> Result<Vec<String>, DriverError> {
    Ok(sqlx::query(
        "SELECT CAST(column_name AS CHAR)
         FROM information_schema.key_column_usage
         WHERE table_schema = ? AND table_name = ? AND constraint_name = 'PRIMARY'
         ORDER BY ordinal_position",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?
    .into_iter()
    .map(|row| row.get::<String, _>(0))
    .collect())
}

fn exposed_keys(parsed: &view_definition::ParsedView, primary_key: &[String]) -> Option<Vec<String>> {
    primary_key
        .iter()
        .map(|key| {
            parsed
                .columns
                .iter()
                .find(|column| {
                    column
                        .origin
                        .as_ref()
                        .is_some_and(|(table, name)| *table == parsed.base_table && name == key)
                })
                .map(|column| column.alias.clone())
        })
        .collect()
}
