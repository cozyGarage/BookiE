use std::collections::HashSet;

use bookie_core::{DriverError, Value, ViewEditability};

use crate::{MssqlConnection, as_text, text_param};

pub(super) async fn fetch(
    connection: &MssqlConnection,
    schema: Option<&str>,
    view: &str,
) -> Result<ViewEditability, DriverError> {
    let mut client = connection.client().await?;
    let exists = connection
        .query_result(
            &mut client,
            "SELECT s.name FROM sys.views v JOIN sys.schemas s ON s.schema_id = v.schema_id \
             WHERE v.name = @P1 AND s.name = COALESCE(@P2, SCHEMA_NAME())",
            &[text_param(view), schema.map_or(Value::Null, text_param)],
            1,
        )
        .await?;
    if exists.rows.is_empty() {
        return Ok(ViewEditability::NotApplicable);
    }
    let source_schema = as_text(exists.rows.first().and_then(|row| row.first())).unwrap_or_else(|| "dbo".into());

    let source_query = format!(
        "SELECT * FROM {}.{}",
        quote_identifier(&source_schema),
        quote_identifier(view)
    );
    let description = connection
        .query_result(
            &mut client,
            "SELECT name, source_schema_name, source_table_name, source_column_name, \
                    is_updateable, is_computed_column \
             FROM sys.dm_exec_describe_first_result_set(@P1, NULL, 1) \
             WHERE error_number IS NULL AND is_hidden = 0 ORDER BY column_ordinal",
            &[text_param(&source_query)],
            bookie_core::MAX_QUERY_ROWS,
        )
        .await?;
    let columns = description
        .rows
        .iter()
        .filter_map(|row| {
            Some(Origin {
                name: as_text(row.first())?,
                schema: as_text(row.get(1)),
                table: as_text(row.get(2)),
                column: as_text(row.get(3)),
                updateable: is_true(row.get(4)),
                computed: is_true(row.get(5)),
            })
        })
        .collect::<Vec<_>>();
    if columns.is_empty() || !columns.iter().any(|column| column.updateable) {
        return Ok(ViewEditability::NotUpdatable);
    }

    let base_tables = columns
        .iter()
        .filter_map(|column| Some((column.schema.as_ref()?, column.table.as_ref()?)))
        .collect::<HashSet<_>>();
    if base_tables.len() != 1 {
        return Ok(ViewEditability::NotUpdatable);
    }
    let Some((base_schema, base_table)) = base_tables.iter().next().copied() else {
        return Ok(ViewEditability::NotUpdatable);
    };
    let primary_key = connection
        .query_result(
            &mut client,
            "SELECT c.name FROM sys.indexes i \
             JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
             JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
             JOIN sys.tables t ON t.object_id = i.object_id \
             JOIN sys.schemas s ON s.schema_id = t.schema_id \
             WHERE i.is_primary_key = 1 AND ic.key_ordinal > 0 AND s.name = @P1 AND t.name = @P2 \
             ORDER BY ic.key_ordinal",
            &[text_param(base_schema), text_param(base_table)],
            bookie_core::MAX_QUERY_ROWS,
        )
        .await?
        .rows
        .iter()
        .filter_map(|row| as_text(row.first()))
        .collect::<Vec<_>>();
    if primary_key.is_empty() {
        return Ok(ViewEditability::KeyNotExposed);
    }
    let Some(key_columns) = primary_key
        .iter()
        .map(|key| {
            columns
                .iter()
                .find(|column| column.column.as_ref() == Some(key))
                .map(|column| column.name.clone())
        })
        .collect::<Option<Vec<_>>>()
    else {
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
