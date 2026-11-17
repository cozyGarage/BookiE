use std::collections::HashSet;

use bookie_core::{DriverError, Value, ViewEditability};

use crate::{MssqlConnection, as_text, text_param};

pub(super) async fn fetch(
    connection: &MssqlConnection,
    schema: Option<&str>,
    view: &str,
) -> Result<ViewEditability, DriverError> {
    let mut client = connection.client().await?;
    let Some(source_schema) = view_schema(connection, &mut client, schema, view).await? else {
        return Ok(ViewEditability::NotApplicable);
    };
    let columns = describe_view(connection, &mut client, &source_schema, view).await?;
    if columns.is_empty() || !columns.iter().any(|column| column.updateable) {
        return Ok(ViewEditability::NotUpdatable);
    }
    let Some((base_schema, base_table)) = single_base_table(&columns) else {
        return Ok(ViewEditability::NotUpdatable);
    };
    let primary_key = primary_key(connection, &mut client, base_schema, base_table).await?;
    if primary_key.is_empty() {
        return Ok(ViewEditability::KeyNotExposed);
    }
    let Some(key_columns) = exposed_keys(&columns, &primary_key) else {
        return Ok(ViewEditability::KeyNotExposed);
    };

    Ok(ViewEditability::Editable {
        key_columns,
        read_only_columns: columns
            .iter()
            .filter(|column| column.computed || !column.updateable)
            .map(|column| column.name.clone())
            .collect(),
        insertable: false,
    })
}

async fn view_schema(
    connection: &MssqlConnection,
    client: &mut crate::MssqlClient,
    schema: Option<&str>,
    view: &str,
) -> Result<Option<String>, DriverError> {
    let result = connection
        .query_result(
            client,
            "SELECT s.name FROM sys.views v JOIN sys.schemas s ON s.schema_id = v.schema_id \
             WHERE v.name = @P1 AND s.name = COALESCE(@P2, SCHEMA_NAME())",
            &[text_param(view), schema.map_or(Value::Null, text_param)],
            1,
        )
        .await?;
    Ok(result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(|value| as_text(Some(value))))
}

async fn describe_view(
    connection: &MssqlConnection,
    client: &mut crate::MssqlClient,
    schema: &str,
    view: &str,
) -> Result<Vec<Origin>, DriverError> {
    let source_query = format!("SELECT * FROM {}.{}", quote_identifier(schema), quote_identifier(view));
    let result = connection
        .query_result(
            client,
            "SELECT name, source_schema, source_table, source_column, \
                    is_updateable, is_computed_column \
             FROM sys.dm_exec_describe_first_result_set(@P1, NULL, 1) \
             WHERE error_number IS NULL AND is_hidden = 0 ORDER BY column_ordinal",
            &[text_param(&source_query)],
            bookie_core::MAX_QUERY_ROWS,
        )
        .await?;
    Ok(result.rows.iter().filter_map(origin_from_row).collect())
}

fn origin_from_row(row: &[Value]) -> Option<Origin> {
    Some(Origin {
        name: as_text(row.first())?,
        schema: as_text(row.get(1)),
        table: as_text(row.get(2)),
        column: as_text(row.get(3)),
        updateable: is_true(row.get(4)),
        computed: is_true(row.get(5)),
    })
}

fn single_base_table(columns: &[Origin]) -> Option<(&str, &str)> {
    let base_tables = columns
        .iter()
        .filter_map(|column| Some((column.schema.as_deref()?, column.table.as_deref()?)))
        .collect::<HashSet<_>>();
    if base_tables.len() == 1 {
        base_tables.iter().next().copied()
    } else {
        None
    }
}

async fn primary_key(
    connection: &MssqlConnection,
    client: &mut crate::MssqlClient,
    schema: &str,
    table: &str,
) -> Result<Vec<String>, DriverError> {
    Ok(connection
        .query_result(
            client,
            "SELECT c.name FROM sys.indexes i \
             JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
             JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
             JOIN sys.tables t ON t.object_id = i.object_id \
             JOIN sys.schemas s ON s.schema_id = t.schema_id \
             WHERE i.is_primary_key = 1 AND ic.key_ordinal > 0 AND s.name = @P1 AND t.name = @P2 \
             ORDER BY ic.key_ordinal",
            &[text_param(schema), text_param(table)],
            bookie_core::MAX_QUERY_ROWS,
        )
        .await?
        .rows
        .iter()
        .filter_map(|row| row.first().and_then(|value| as_text(Some(value))))
        .collect())
}

fn exposed_keys(columns: &[Origin], primary_key: &[String]) -> Option<Vec<String>> {
    primary_key
        .iter()
        .map(|key| {
            columns
                .iter()
                .find(|column| column.column.as_ref() == Some(key))
                .map(|column| column.name.clone())
        })
        .collect()
}

struct Origin {
    name: String,
    schema: Option<String>,
    table: Option<String>,
    column: Option<String>,
    updateable: bool,
    computed: bool,
}

fn is_true(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Bool(true) | Value::Int(1)))
}

fn quote_identifier(identifier: &str) -> String {
    format!("[{}]", identifier.replace(']', "]]"))
}
