use bookie_core::{ColumnInfo, DriverError, TableInfo, Value};
use oracledb::Metadata;

use crate::OracleConnection;

pub(crate) const TABLES_SQL: &str = "SELECT t.owner, t.table_name FROM all_tables t \
    JOIN all_users u ON u.username = t.owner WHERE u.oracle_maintained = 'N' \
    AND t.nested = 'NO' AND t.secondary = 'N' ORDER BY t.owner, t.table_name";

pub(crate) const VIEWS_SQL: &str = "SELECT v.owner, v.view_name FROM all_views v \
    JOIN all_users u ON u.username = v.owner WHERE u.oracle_maintained = 'N' ORDER BY v.owner, v.view_name";

const COLUMNS_SQL: &str = "SELECT c.column_name, c.data_type, c.nullable, c.data_default, \
    CASE WHEN EXISTS (SELECT 1 FROM all_constraints k JOIN all_cons_columns kc \
        ON kc.owner = k.owner AND kc.constraint_name = k.constraint_name \
        WHERE k.constraint_type = 'P' AND k.owner = c.owner AND k.table_name = c.table_name \
        AND kc.column_name = c.column_name) THEN 1 ELSE 0 END, \
    c.identity_column, c.virtual_column, cc.comments \
    FROM all_tab_cols c LEFT JOIN all_col_comments cc \
        ON cc.owner = c.owner AND cc.table_name = c.table_name AND cc.column_name = c.column_name \
    WHERE c.owner = :1 AND c.table_name = :2 AND c.hidden_column = 'NO' ORDER BY c.column_id";

pub(crate) fn column_info(column: &Metadata) -> ColumnInfo {
    ColumnInfo {
        name: column.name().to_string(),
        data_type: column.data_type(),
        nullable: column.nullable(),
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

pub(crate) async fn list(connection: &OracleConnection, sql: &str) -> Result<Vec<TableInfo>, DriverError> {
    let result = connection.read_query(sql, Vec::new(), usize::MAX).await?;
    Ok(result
        .rows
        .iter()
        .filter_map(|row| match (row.first(), row.get(1)) {
            (Some(Value::Text(schema)), Some(Value::Text(name))) => Some(TableInfo {
                schema: Some(schema.clone()),
                name: name.clone(),
            }),
            _ => None,
        })
        .collect())
}

pub(crate) async fn columns(
    connection: &OracleConnection,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<ColumnInfo>, DriverError> {
    let owner = match schema {
        Some(schema) => schema.to_string(),
        None => current_schema(connection).await?,
    };
    let result = connection
        .read_query(COLUMNS_SQL, vec![owner, table.to_string()], usize::MAX)
        .await?;
    Ok(result.rows.iter().map(declared_column).collect())
}

async fn current_schema(connection: &OracleConnection) -> Result<String, DriverError> {
    let result = connection
        .read_query(
            "SELECT sys_context('USERENV', 'CURRENT_SCHEMA') FROM dual",
            Vec::new(),
            1,
        )
        .await?;
    match result.rows.iter().next().and_then(|row| row.first()) {
        Some(Value::Text(schema)) => Ok(schema.clone()),
        _ => Err(DriverError::Internal("oracle did not report the current schema".into())),
    }
}

fn text(row: &[Value], index: usize) -> Option<String> {
    match row.get(index) {
        Some(Value::Text(value)) => Some(value.clone()),
        _ => None,
    }
}

fn declared_column(row: &[Value]) -> ColumnInfo {
    ColumnInfo {
        name: text(row, 0).unwrap_or_default(),
        data_type: text(row, 1).unwrap_or_default(),
        nullable: text(row, 2).as_deref() == Some("Y"),
        primary_key: matches!(row.get(4), Some(Value::Decimal(flag)) if !flag.is_zero()),
        is_auto_increment: text(row, 5).as_deref() == Some("YES"),
        default_value: text(row, 3).map(|value| value.trim().to_string()),
        is_generated: text(row, 6).as_deref() == Some("YES"),
        comment: text(row, 7),
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}
