use bookie_core::{ColumnInfo, DriverError, TableInfo};

use crate::{CassandraConnection, text};

const SYSTEM_KEYSPACES: [&str; 10] = [
    "system",
    "system_schema",
    "system_auth",
    "system_distributed",
    "system_traces",
    "system_views",
    "system_virtual_schema",
    "system_replicated_keys",
    "system_auth_v2",
    "system_distributed_everywhere",
];

pub(crate) async fn tables(connection: &CassandraConnection, cql: &str) -> Result<Vec<TableInfo>, DriverError> {
    let result = connection.read(cql, &[], usize::MAX).await?;
    let mut found: Vec<TableInfo> = result
        .rows
        .iter()
        .filter_map(|row| Some((text(row.first()?)?, text(row.get(1)?)?)))
        .filter(|(keyspace, _)| !SYSTEM_KEYSPACES.contains(&keyspace.as_str()))
        .map(|(keyspace, name)| TableInfo {
            schema: Some(keyspace),
            name,
        })
        .collect();
    found.sort_by(|left, right| (&left.schema, &left.name).cmp(&(&right.schema, &right.name)));
    Ok(found)
}

pub(crate) const TABLES_CQL: &str = "SELECT keyspace_name, table_name FROM system_schema.tables";
pub(crate) const VIEWS_CQL: &str = "SELECT keyspace_name, view_name FROM system_schema.views";

pub(crate) async fn columns(
    connection: &CassandraConnection,
    keyspace: Option<&str>,
    table: &str,
) -> Result<Vec<ColumnInfo>, DriverError> {
    let keyspace = keyspace
        .or(connection.keyspace.as_deref())
        .ok_or_else(|| DriverError::Unsupported("a keyspace is needed to read a table's columns".into()))?;
    let cql = format!(
        "SELECT column_name, type, kind, position FROM system_schema.columns \
         WHERE keyspace_name = {} AND table_name = {}",
        literal(keyspace),
        literal(table)
    );
    let result = connection.read(&cql, &[], usize::MAX).await?;
    let mut columns: Vec<(u8, i64, ColumnInfo)> = result
        .rows
        .iter()
        .filter_map(|row| {
            let name = text(row.first()?)?;
            let data_type = text(row.get(1)?)?;
            let kind = text(row.get(2)?)?;
            let position = match row.get(3) {
                Some(bookie_core::Value::Int(position)) => *position,
                _ => 0,
            };
            let primary_key = matches!(kind.as_str(), "partition_key" | "clustering");
            let order = match kind.as_str() {
                "partition_key" => 0,
                "clustering" => 1,
                "static" => 2,
                _ => 3,
            };
            Some((
                order,
                position,
                ColumnInfo {
                    name,
                    data_type,
                    nullable: !primary_key,
                    primary_key,
                    is_auto_increment: false,
                    default_value: None,
                    is_generated: false,
                    comment: None,
                    collation: None,
                    enum_type: None,
                    domain_type: None,
                },
            ))
        })
        .collect();
    columns.sort_by(|left, right| (left.0, left.1, &left.2.name).cmp(&(right.0, right.1, &right.2.name)));
    Ok(columns.into_iter().map(|(_, _, column)| column).collect())
}

fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_literals_double_their_quotes() {
        assert_eq!(literal("o'brien"), "'o''brien'");
    }
}
