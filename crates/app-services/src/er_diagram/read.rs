use bookie_core::{
    ColumnInfo, Connection, DriverCapabilities, DriverError, ForeignKeyInfo, OperationControl, TableInfo,
    check_pre_dispatch,
};

use super::model::TableRead;

pub const MAX_TABLES: usize = 150;
pub const MAX_SCHEMA_METADATA_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct SchemaRead {
    pub reads: Vec<TableRead>,
    pub total_tables: usize,
    pub foreign_keys_supported: bool,
    pub skipped: Vec<String>,
    pub metadata_limit_reached: bool,
}

impl SchemaRead {
    pub fn truncated(&self) -> bool {
        self.metadata_limit_reached || self.total_tables > self.reads.len() + self.skipped.len()
    }
}

pub async fn read_schema(
    connection: &dyn Connection,
    schema: Option<&str>,
    tables: &[TableInfo],
    capabilities: DriverCapabilities,
    control: &OperationControl,
    progress: impl FnMut(usize, usize),
) -> Result<SchemaRead, DriverError> {
    read_schema_with_limit(
        connection,
        schema,
        tables,
        capabilities,
        control,
        MAX_SCHEMA_METADATA_BYTES,
        progress,
    )
    .await
}

pub(super) async fn read_schema_with_limit(
    connection: &dyn Connection,
    schema: Option<&str>,
    tables: &[TableInfo],
    capabilities: DriverCapabilities,
    control: &OperationControl,
    metadata_byte_limit: usize,
    mut progress: impl FnMut(usize, usize),
) -> Result<SchemaRead, DriverError> {
    let mut tables: Vec<_> = tables.iter().collect();
    tables.sort_unstable_by(|a, b| a.name.cmp(&b.name).then_with(|| a.schema.cmp(&b.schema)));
    let total_tables = tables.len();
    tables.truncate(MAX_TABLES);
    let mut read = SchemaRead {
        total_tables,
        foreign_keys_supported: capabilities.supports_foreign_key_metadata,
        ..SchemaRead::default()
    };
    let wanted = tables.len();
    let mut metadata_bytes = 0usize;
    for (done, table) in tables.into_iter().enumerate() {
        check_pre_dispatch(control)?;
        let table_schema = table.schema.as_deref().or(schema);
        match read_table(connection, table_schema, &table.name, capabilities, control).await {
            Ok(table_read) => {
                let table_bytes = estimated_table_bytes(&table_read);
                if metadata_bytes.saturating_add(table_bytes) > metadata_byte_limit {
                    read.metadata_limit_reached = true;
                    break;
                }
                metadata_bytes = metadata_bytes.saturating_add(table_bytes);
                read.reads.push(table_read);
            }
            Err(
                error @ (DriverError::Cancelled
                | DriverError::TimedOut
                | DriverError::Disconnected
                | DriverError::ConnectionRefused
                | DriverError::AuthFailed
                | DriverError::Tls(_)
                | DriverError::PolicyDenied(_)),
            ) => return Err(error),
            Err(_) => read.skipped.push(table.name.clone()),
        }
        progress(done + 1, wanted);
    }
    Ok(read)
}

fn estimated_table_bytes(table: &TableRead) -> usize {
    let mut bytes = std::mem::size_of::<TableRead>()
        .saturating_add(table.schema.as_ref().map_or(0, String::capacity))
        .saturating_add(table.name.capacity())
        .saturating_add(
            table
                .columns
                .capacity()
                .saturating_mul(std::mem::size_of::<ColumnInfo>()),
        )
        .saturating_add(
            table
                .foreign_keys
                .capacity()
                .saturating_mul(std::mem::size_of::<ForeignKeyInfo>()),
        );
    for column in &table.columns {
        bytes = bytes
            .saturating_add(column.name.capacity())
            .saturating_add(column.data_type.capacity())
            .saturating_add(column.default_value.as_ref().map_or(0, String::capacity))
            .saturating_add(column.comment.as_ref().map_or(0, String::capacity))
            .saturating_add(column.collation.as_ref().map_or(0, String::capacity));
        for qualified in [column.enum_type.as_ref(), column.domain_type.as_ref()]
            .into_iter()
            .flatten()
        {
            bytes = bytes
                .saturating_add(qualified.schema.capacity())
                .saturating_add(qualified.name.capacity());
        }
    }
    for key in &table.foreign_keys {
        bytes = bytes
            .saturating_add(key.name.capacity())
            .saturating_add(key.ref_schema.as_ref().map_or(0, String::capacity))
            .saturating_add(key.ref_table.capacity())
            .saturating_add(key.on_delete.as_ref().map_or(0, String::capacity))
            .saturating_add(key.on_update.as_ref().map_or(0, String::capacity))
            .saturating_add(key.columns.capacity().saturating_mul(std::mem::size_of::<String>()))
            .saturating_add(key.ref_columns.capacity().saturating_mul(std::mem::size_of::<String>()));
        bytes = key
            .columns
            .iter()
            .chain(&key.ref_columns)
            .fold(bytes, |total, name| total.saturating_add(name.capacity()));
    }
    bytes
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
