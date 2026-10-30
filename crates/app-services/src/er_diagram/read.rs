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
    let mut names: Vec<&str> = tables.iter().map(|table| table.name.as_str()).collect();
    names.sort_unstable();
    let total_tables = names.len();
    names.truncate(MAX_TABLES);
    let mut read = SchemaRead {
        total_tables,
        foreign_keys_supported: capabilities.supports_foreign_key_metadata,
        ..SchemaRead::default()
    };
    let wanted = names.len();
    for (done, name) in names.into_iter().enumerate() {
        check_pre_dispatch(control)?;
        match read_table(connection, schema, name, capabilities, control).await {
            Ok(table) => read.reads.push(table),
            Err(error @ (DriverError::Cancelled | DriverError::TimedOut)) => return Err(error),
            Err(_) => read.skipped.push(name.to_string()),
        }
        progress(done + 1, wanted);
    }
    Ok(read)
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
    Ok(TableRead {
        schema: schema.map(str::to_string),
        name: name.to_string(),
        columns,
        foreign_keys,
    })
}
