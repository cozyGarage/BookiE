use std::collections::HashSet;

use bookie_core::{DriverError, ViewEditability};
use sqlx::{AssertSqlSafe, Column, Executor, MySqlConnection, Row, SqlSafeStr, Statement};

use crate::map_sqlx_error;

pub(super) async fn fetch(
    connection: &mut MySqlConnection,
    schema: Option<&str>,
    view: &str,
) -> Result<ViewEditability, DriverError> {
    let Some((schema, updatable)) = view_info(connection, schema, view).await? else {
        return Ok(ViewEditability::NotApplicable);
    };
    if !updatable {
        return Ok(ViewEditability::NotUpdatable);
    }
    let description = describe_view(connection, &schema, view).await?;
    let Some(base_table) = single_base_table(&description.origins) else {
        return Ok(ViewEditability::NotUpdatable);
    };
    let Some((base_schema, base_name)) = base_table.rsplit_once('.') else {
        return Ok(ViewEditability::NotUpdatable);
    };
    let primary_key = primary_key(connection, base_schema, base_name).await?;
    if primary_key.is_empty() {
        return Ok(ViewEditability::KeyNotExposed);
    }
    let Some(key_columns) = exposed_keys(&description.origins, base_table, &primary_key) else {
        return Ok(ViewEditability::KeyNotExposed);
    };

    Ok(ViewEditability::Editable {
        key_columns,
        read_only_columns: description.read_only_columns,
        insertable: false,
    })
}

async fn view_info(
    connection: &mut MySqlConnection,
    schema: Option<&str>,
    view: &str,
) -> Result<Option<(String, bool)>, DriverError> {
    Ok(sqlx::query(
        "SELECT CAST(table_schema AS CHAR), is_updatable
         FROM information_schema.views
         WHERE table_schema = COALESCE(?, DATABASE()) AND table_name = ?",
    )
    .bind(schema)
    .bind(view)
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_sqlx_error)?
    .map(|row| (row.get::<String, _>(0), row.get::<String, _>(1) == "YES")))
}

async fn describe_view(connection: &mut MySqlConnection, schema: &str, view: &str) -> Result<ViewColumns, DriverError> {
    let sql = format!(
        "SELECT * FROM `{}`.`{}` LIMIT 0",
        schema.replace('`', "``"),
        view.replace('`', "``")
    );
    let description = connection
        .prepare_with(AssertSqlSafe(sql.as_str()).into_sql_str(), &[])
        .await
        .map_err(map_sqlx_error)?;
    let mut metadata = ViewColumns::default();
    for column in description.columns() {
        if let Some(origin) = column.origin().table_column() {
            metadata.origins.push(Origin {
                name: column.name().to_owned(),
                table: origin.table.to_string(),
                column: origin.name.to_string(),
            });
        } else {
            metadata.read_only_columns.push(column.name().to_owned());
        }
    }
    Ok(metadata)
}

fn single_base_table(origins: &[Origin]) -> Option<&str> {
    let tables = origins
        .iter()
        .map(|origin| origin.table.as_str())
        .collect::<HashSet<_>>();
    if tables.len() == 1 {
        tables.iter().next().copied()
    } else {
        None
    }
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

fn exposed_keys(origins: &[Origin], base_table: &str, primary_key: &[String]) -> Option<Vec<String>> {
    primary_key
        .iter()
        .map(|key| {
            origins
                .iter()
                .find(|origin| origin.table == base_table && origin.column == *key)
                .map(|origin| origin.name.clone())
        })
        .collect()
}

#[derive(Default)]
struct ViewColumns {
    origins: Vec<Origin>,
    read_only_columns: Vec<String>,
}

struct Origin {
    name: String,
    table: String,
    column: String,
}
