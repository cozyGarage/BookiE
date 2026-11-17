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
    let definition = view_definition(connection, &mut client, &source_schema, view).await?;
    if !definition.as_deref().is_some_and(definition_allows_updates) {
        return Ok(ViewEditability::NotUpdatable);
    }
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

async fn view_definition(
    connection: &MssqlConnection,
    client: &mut crate::MssqlClient,
    schema: &str,
    view: &str,
) -> Result<Option<String>, DriverError> {
    let result = connection
        .query_result(
            client,
            "SELECT m.definition FROM sys.sql_modules m \
             JOIN sys.views v ON v.object_id = m.object_id \
             JOIN sys.schemas s ON s.schema_id = v.schema_id \
             WHERE v.name = @P1 AND s.name = @P2",
            &[text_param(view), text_param(schema)],
            1,
        )
        .await?;
    Ok(result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(|value| as_text(Some(value))))
}

const BLOCKING_WORDS: [&str; 15] = [
    "GROUP",
    "DISTINCT",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "HAVING",
    "TOP",
    "OVER",
    "PIVOT",
    "UNPIVOT",
    "APPLY",
    "OFFSET",
    "TABLESAMPLE",
    "FETCH",
    "WITHIN",
];

const AGGREGATE_FUNCTIONS: [&str; 14] = [
    "COUNT",
    "COUNT_BIG",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "STDEV",
    "STDEVP",
    "VAR",
    "VARP",
    "STRING_AGG",
    "CHECKSUM_AGG",
    "GROUPING",
    "GROUPING_ID",
];

fn definition_allows_updates(definition: &str) -> bool {
    let words = sql_words(definition);
    !words.iter().enumerate().any(|(index, word)| {
        BLOCKING_WORDS.contains(&word.as_str())
            || (AGGREGATE_FUNCTIONS.contains(&word.as_str()) && words.get(index + 1).is_some_and(|next| next == "("))
    })
}

fn sql_words(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut words = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        if current == '-' && next == Some('-') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
        } else if current == '/' && next == Some('*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index += 2;
        } else if matches!(current, '\'' | '"' | '[') {
            let close = if current == '[' { ']' } else { current };
            index += 1;
            while index < chars.len() {
                if chars[index] == close {
                    if chars.get(index + 1) == Some(&close) {
                        index += 2;
                        continue;
                    }
                    break;
                }
                index += 1;
            }
            index += 1;
        } else if current.is_alphanumeric() || current == '_' {
            let start = index;
            while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_') {
                index += 1;
            }
            words.push(chars[start..index].iter().collect::<String>().to_uppercase());
        } else {
            if current == '(' {
                words.push("(".into());
            }
            index += 1;
        }
    }
    words
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

#[cfg(test)]
mod definition_tests {
    use super::definition_allows_updates;

    #[test]
    fn plain_projections_stay_editable() {
        for sql in [
            "CREATE VIEW v AS SELECT id AS item_id, part, name, UPPER(name) AS upper_name FROM dbo.t",
            "CREATE VIEW v AS SELECT id, name FROM dbo.t WHERE name <> 'GROUP BY' -- DISTINCT",
            "CREATE VIEW v AS SELECT [count], \"top\" FROM dbo.t /* UNION */",
            "CREATE VIEW v AS SELECT id, name FROM dbo.t WITH CHECK OPTION",
        ] {
            assert!(definition_allows_updates(sql), "{sql}");
        }
    }

    #[test]
    fn shapes_sql_server_will_not_update_through_are_refused() {
        for sql in [
            "CREATE VIEW v AS SELECT id, COUNT(*) AS total FROM dbo.t GROUP BY id",
            "CREATE VIEW v AS SELECT DISTINCT id FROM dbo.t",
            "CREATE VIEW v AS SELECT id FROM dbo.a UNION ALL SELECT id FROM dbo.b",
            "CREATE VIEW v AS SELECT TOP 10 id FROM dbo.t",
            "CREATE VIEW v AS SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS n FROM dbo.t",
            "CREATE VIEW v AS SELECT id, max (name) FROM dbo.t",
            "CREATE VIEW v AS SELECT t.id FROM dbo.t CROSS APPLY dbo.f(t.id) AS x",
        ] {
            assert!(!definition_allows_updates(sql), "{sql}");
        }
    }
}
