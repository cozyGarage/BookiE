use tablepro_core::{Connection, DriverCapabilities, DriverError, OperationControl, TableInfo, check_pre_dispatch};

use super::model::TableRead;

pub const MAX_TABLES: usize = 150;

#[derive(Debug, Default)]
pub struct SchemaRead {
    pub reads: Vec<TableRead>,
    pub total_tables: usize,
    pub foreign_keys_supported: bool,
    pub skipped: Vec<String>,
}

impl SchemaRead {
    pub fn truncated(&self) -> bool {
        self.total_tables > self.reads.len() + self.skipped.len()
    }
}

pub async fn read_schema(
    connection: &dyn Connection,
    schema: Option<&str>,
    tables: &[TableInfo],
    capabilities: DriverCapabilities,
    control: &OperationControl,
    mut progress: impl FnMut(usize, usize),
) -> Result<SchemaRead, DriverError> {
    let mut tables: Vec<&TableInfo> = tables.iter().collect();
    tables.sort_unstable_by(|left, right| left.schema.cmp(&right.schema).then_with(|| left.name.cmp(&right.name)));
    let total_tables = tables.len();
    tables.truncate(MAX_TABLES);
    let mut read = SchemaRead {
        total_tables,
        foreign_keys_supported: capabilities.supports_foreign_key_metadata,
        ..SchemaRead::default()
    };
    let wanted = tables.len();
    for (done, table) in tables.into_iter().enumerate() {
        check_pre_dispatch(control)?;
        let table_schema = schema.or(table.schema.as_deref());
        match read_table(connection, table_schema, &table.name, capabilities, control).await {
            Ok(table) => read.reads.push(table),
            Err(error @ (DriverError::Cancelled | DriverError::TimedOut)) => return Err(error),
            Err(_) => read.skipped.push(table_label(table_schema, &table.name)),
        }
        progress(done + 1, wanted);
    }
    Ok(read)
}

fn table_label(schema: Option<&str>, name: &str) -> String {
    schema.map_or_else(|| name.to_string(), |schema| format!("{schema}.{name}"))
}

async fn read_table(
    connection: &dyn Connection,
    schema: Option<&str>,
    name: &str,
    capabilities: DriverCapabilities,
    control: &OperationControl,
) -> Result<TableRead, DriverError> {
    let columns = connection.fetch_columns_controlled(schema, name, control).await?;
    let foreign_keys = if capabilities.supports_foreign_key_metadata {
        connection.fetch_foreign_keys_controlled(schema, name, control).await?
    } else {
        Vec::new()
    };
    let indexes = if capabilities.supports_index_metadata && !foreign_keys.is_empty() {
        connection.fetch_indexes_controlled(schema, name, control).await?
    } else {
        Vec::new()
    };
    Ok(TableRead {
        schema: schema.map(str::to_string),
        name: name.to_string(),
        columns,
        foreign_keys,
        indexes,
    })
}
