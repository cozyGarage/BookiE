use bookie_core::{
    ColumnInfo, Connection, DriverCapabilities, DriverError, OperationControl, QueryResult, QueryRows, TableInfo,
    Value, check_pre_dispatch,
};
use serde_json::json;

const MAX_ROWS: usize = 100_000;

pub async fn load(
    connection: &dyn Connection,
    schema: Option<&str>,
    tables: &[TableInfo],
    capabilities: DriverCapabilities,
    control: &OperationControl,
) -> Result<QueryResult, DriverError> {
    check_pre_dispatch(control)?;
    let mut tables = tables.to_vec();
    tables.sort_unstable_by(|a, b| a.name.cmp(&b.name).then_with(|| a.schema.cmp(&b.schema)));
    let mut rows = Vec::new();
    for batch in tables.chunks(crate::er_diagram::read::MAX_TABLES) {
        check_pre_dispatch(control)?;
        let read =
            crate::er_diagram::read::read_schema(connection, schema, batch, capabilities, control, |_, _| {}).await?;
        append_read(connection, &read, capabilities, control, &mut rows).await?;
    }
    Ok(result(rows))
}

fn result(rows: Vec<Vec<Value>>) -> QueryResult {
    QueryResult {
        columns: columns(),
        rows: QueryRows::from_rows(rows),
        truncated: false,
    }
}

async fn append_read(
    connection: &dyn Connection,
    read: &crate::er_diagram::read::SchemaRead,
    capabilities: DriverCapabilities,
    control: &OperationControl,
    rows: &mut Vec<Vec<Value>>,
) -> Result<(), DriverError> {
    if read.truncated() || !read.skipped.is_empty() {
        return Err(DriverError::Internal(
            "catalog metadata is incomplete; no data dictionary was created".into(),
        ));
    }
    for table in &read.reads {
        check_pre_dispatch(control)?;
        append_table_rows(connection, table, capabilities, control, rows).await?;
    }
    Ok(())
}

async fn append_table_rows(
    connection: &dyn Connection,
    table: &crate::er_diagram::TableRead,
    capabilities: DriverCapabilities,
    control: &OperationControl,
    rows: &mut Vec<Vec<Value>>,
) -> Result<(), DriverError> {
    let indexes = if capabilities.supports_index_metadata {
        connection
            .fetch_indexes_controlled(table.schema.as_deref(), &table.name, control)
            .await?
    } else {
        Vec::new()
    };
    push_table_row(rows, table, capabilities)?;
    append_column_rows(rows, table, control)?;
    append_index_rows(rows, table, indexes)?;
    append_foreign_key_rows(rows, table)
}

fn push_table_row(
    rows: &mut Vec<Vec<Value>>,
    table: &crate::er_diagram::TableRead,
    capabilities: DriverCapabilities,
) -> Result<(), DriverError> {
    push_row(
        rows,
        dictionary_row(DictionaryRow {
            schema: table.schema.as_deref(),
            table: &table.name,
            kind: "table",
            name: &table.name,
            data_type: None,
            nullable: None,
            default: None,
            primary_key: None,
            comment: None,
            details: json!({
                "indexes_supported": capabilities.supports_index_metadata,
                "foreign_keys_supported": capabilities.supports_foreign_key_metadata,
            }),
        }),
    )
}

fn append_column_rows(
    rows: &mut Vec<Vec<Value>>,
    table: &crate::er_diagram::TableRead,
    control: &OperationControl,
) -> Result<(), DriverError> {
    for column in &table.columns {
        check_pre_dispatch(control)?;
        push_row(
            rows,
            dictionary_row(DictionaryRow {
                schema: table.schema.as_deref(),
                table: &table.name,
                kind: "column",
                name: &column.name,
                data_type: Some(Value::Text(column.data_type.clone())),
                nullable: Some(Value::Bool(column.nullable)),
                default: column.default_value.clone().map(Value::Text),
                primary_key: Some(Value::Bool(column.primary_key)),
                comment: column.comment.clone().map(Value::Text),
                details: json!({
                    "auto_increment": column.is_auto_increment,
                    "generated": column.is_generated,
                    "collation": column.collation,
                }),
            }),
        )?;
    }
    Ok(())
}

fn append_index_rows(
    rows: &mut Vec<Vec<Value>>,
    table: &crate::er_diagram::TableRead,
    indexes: Vec<bookie_core::IndexInfo>,
) -> Result<(), DriverError> {
    for index in indexes {
        push_row(
            rows,
            dictionary_row(DictionaryRow {
                schema: table.schema.as_deref(),
                table: &table.name,
                kind: "index",
                name: &index.name,
                data_type: None,
                nullable: None,
                default: None,
                primary_key: None,
                comment: None,
                details: json!({
                    "columns": index.columns,
                    "unique": index.unique,
                    "primary": index.primary,
                    "predicate": index.predicate,
                    "constraint": index.constraint,
                    "has_expression_key": index.has_expression_key,
                    "using": index.using_clause,
                }),
            }),
        )?;
    }
    Ok(())
}

fn append_foreign_key_rows(
    rows: &mut Vec<Vec<Value>>,
    table: &crate::er_diagram::TableRead,
) -> Result<(), DriverError> {
    for foreign_key in &table.foreign_keys {
        push_row(
            rows,
            dictionary_row(DictionaryRow {
                schema: table.schema.as_deref(),
                table: &table.name,
                kind: "foreign_key",
                name: &foreign_key.name,
                data_type: None,
                nullable: None,
                default: None,
                primary_key: None,
                comment: None,
                details: json!({
                    "columns": foreign_key.columns,
                    "ref_schema": foreign_key.ref_schema,
                    "ref_table": foreign_key.ref_table,
                    "ref_columns": foreign_key.ref_columns,
                    "on_delete": foreign_key.on_delete,
                    "on_update": foreign_key.on_update,
                }),
            }),
        )?;
    }
    Ok(())
}

struct DictionaryRow<'a> {
    schema: Option<&'a str>,
    table: &'a str,
    kind: &'a str,
    name: &'a str,
    data_type: Option<Value>,
    nullable: Option<Value>,
    default: Option<Value>,
    primary_key: Option<Value>,
    comment: Option<Value>,
    details: serde_json::Value,
}

fn dictionary_row(row: DictionaryRow<'_>) -> Vec<Value> {
    vec![
        row.schema.map_or(Value::Null, |value| Value::Text(value.into())),
        Value::Text(row.table.into()),
        Value::Text(row.kind.into()),
        Value::Text(row.name.into()),
        row.data_type.unwrap_or(Value::Null),
        row.nullable.unwrap_or(Value::Null),
        row.default.unwrap_or(Value::Null),
        row.primary_key.unwrap_or(Value::Null),
        row.comment.unwrap_or(Value::Null),
        Value::Json(Box::new(row.details)),
    ]
}

fn push_row(rows: &mut Vec<Vec<Value>>, row: Vec<Value>) -> Result<(), DriverError> {
    if rows.len() == MAX_ROWS {
        return Err(DriverError::Internal(
            "data dictionary exceeds the 100,000 metadata row limit".into(),
        ));
    }
    rows.push(row);
    Ok(())
}

fn columns() -> Vec<ColumnInfo> {
    [
        ("schema", "text"),
        ("table", "text"),
        ("kind", "text"),
        ("name", "text"),
        ("data_type", "text"),
        ("nullable", "boolean"),
        ("default", "text"),
        ("primary_key", "boolean"),
        ("comment", "text"),
        ("details", "json"),
    ]
    .into_iter()
    .map(|(name, data_type)| ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{MAX_ROWS, load};
    use bookie_core::{ConnectOptions, DatabaseDriver, OperationControl, Value};

    #[tokio::test]
    async fn sqlite_dictionary_preserves_structure_and_key_metadata() {
        let connection = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions::default())
            .await
            .unwrap();
        connection.execute("PRAGMA foreign_keys = ON").await.unwrap();
        connection
            .execute("CREATE TABLE parent (id INTEGER PRIMARY KEY, label TEXT NOT NULL DEFAULT 'a,b')")
            .await
            .unwrap();
        connection
            .execute("CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id))")
            .await
            .unwrap();
        connection
            .execute("CREATE INDEX child_parent_idx ON child(parent_id)")
            .await
            .unwrap();
        let tables = connection.list_tables().await.unwrap();
        let result = load(
            connection.as_ref(),
            None,
            &tables,
            drivers_sqlite::SqliteDriver.capabilities(),
            &OperationControl::with_timeout(std::time::Duration::from_secs(5)),
        )
        .await
        .unwrap();

        let label = result.rows.iter().find(|row| {
            matches!(&row[2], Value::Text(kind) if kind == "column")
                && matches!(&row[3], Value::Text(name) if name == "label")
        });
        assert!(label.is_some(), "the dictionary must include real column metadata");
        let label = label.unwrap();
        assert_eq!(label[4], Value::Text("TEXT".into()));
        assert_eq!(label[5], Value::Bool(false));
        assert_eq!(label[6], Value::Text("'a,b'".into()));

        assert!(result.rows.iter().any(|row| {
            matches!(&row[2], Value::Text(kind) if kind == "index")
                && matches!(&row[3], Value::Text(name) if name == "child_parent_idx")
        }));
        assert!(result.rows.iter().any(|row| {
            matches!(&row[2], Value::Text(kind) if kind == "foreign_key")
                && matches!(&row[1], Value::Text(table) if table == "child")
        }));
        assert!(!result.truncated);
    }

    #[tokio::test]
    async fn cancelled_dictionary_read_returns_cancellation() {
        let connection = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions::default())
            .await
            .unwrap();
        let cancellation = tokio_util::sync::CancellationToken::new();
        cancellation.cancel();
        let result = load(
            connection.as_ref(),
            None,
            &[],
            drivers_sqlite::SqliteDriver.capabilities(),
            &OperationControl::new(cancellation, None),
        )
        .await;
        assert!(matches!(result, Err(bookie_core::DriverError::Cancelled)));
    }

    #[tokio::test]
    async fn dictionary_reads_tables_past_the_diagram_batch_limit() {
        let connection = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions::default())
            .await
            .unwrap();
        for index in 0..=crate::er_diagram::read::MAX_TABLES {
            connection
                .execute(&format!("CREATE TABLE table_{index} (id INTEGER)"))
                .await
                .unwrap();
        }
        let tables = connection.list_tables().await.unwrap();
        let result = load(
            connection.as_ref(),
            None,
            &tables,
            bookie_core::DriverCapabilities::default(),
            &OperationControl::with_timeout(std::time::Duration::from_secs(5)),
        )
        .await;
        let result = result.unwrap();
        assert_eq!(
            result
                .rows
                .iter()
                .filter(|row| matches!(&row[2], Value::Text(kind) if kind == "table"))
                .count(),
            crate::er_diagram::read::MAX_TABLES + 1
        );
        let table_names = result
            .rows
            .iter()
            .filter_map(|row| match (&row[2], &row[3]) {
                (Value::Text(kind), Value::Text(name)) if kind == "table" => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(table_names.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn dictionary_rejects_rows_past_its_output_limit() {
        let mut rows = (0..MAX_ROWS).map(|_| Vec::new()).collect::<Vec<_>>();
        assert!(matches!(
            super::push_row(&mut rows, Vec::new()),
            Err(bookie_core::DriverError::Internal(_))
        ));
        assert_eq!(rows.len(), MAX_ROWS);
    }
}
