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
    let mut tables: Vec<_> = tables
        .iter()
        .map(|table| TableInfo {
            schema: table.schema.clone().or_else(|| schema.map(str::to_string)),
            name: table.name.clone(),
        })
        .collect();
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
    let mut next_table = 0;
    let mut fatal = None;
    let (progress_sender, mut progress_receiver) = tokio::sync::mpsc::unbounded_channel();
    let batch_result = {
        let mut visit = |index: usize, result: Result<(Vec<ColumnInfo>, Vec<ForeignKeyInfo>), DriverError>| {
            next_table = index + 1;
            let table = &tables[index];
            let result = result.map(|(columns, foreign_keys)| TableRead {
                schema: table.schema.clone(),
                name: table.name.clone(),
                columns,
                foreign_keys,
            });
            match record_table(&mut read, table, result, metadata_byte_limit, &mut metadata_bytes) {
                Ok(Some(done)) => {
                    let _ = progress_sender.send(done);
                    true
                }
                Ok(None) => false,
                Err(error) => {
                    fatal = Some(error);
                    false
                }
            }
        };
        let mut batch = Box::pin(connection.fetch_schema_table_metadata_controlled(
            &tables,
            capabilities.supports_foreign_key_metadata,
            control,
            &mut visit,
        ));
        loop {
            tokio::select! {
                result = &mut batch => break result,
                Some(done) = progress_receiver.recv() => progress(done, wanted),
            }
        }
    };
    while let Ok(done) = progress_receiver.try_recv() {
        progress(done, wanted);
    }
    if let Some(error) = fatal {
        return Err(error);
    }
    if let Err(error) = batch_result {
        if terminal_metadata_error(&error) {
            return Err(error);
        }
        for table in tables.iter().skip(next_table) {
            check_pre_dispatch(control)?;
            let result = read_table(connection, table.schema.as_deref(), &table.name, capabilities, control).await;
            if record_table(&mut read, table, result, metadata_byte_limit, &mut metadata_bytes)?.is_none() {
                break;
            } else {
                progress(read.reads.len() + read.skipped.len(), wanted);
            }
        }
    }
    Ok(read)
}

fn record_table(
    read: &mut SchemaRead,
    table: &TableInfo,
    result: Result<TableRead, DriverError>,
    byte_limit: usize,
    metadata_bytes: &mut usize,
) -> Result<Option<usize>, DriverError> {
    match result {
        Ok(table_read) => {
            let table_bytes = estimated_table_bytes(&table_read);
            if metadata_bytes.saturating_add(table_bytes) > byte_limit {
                read.metadata_limit_reached = true;
                return Ok(None);
            }
            *metadata_bytes = (*metadata_bytes).saturating_add(table_bytes);
            read.reads.push(table_read);
        }
        Err(error) if terminal_metadata_error(&error) => return Err(error),
        Err(_) => read.skipped.push(table.name.clone()),
    }
    Ok(Some(read.reads.len() + read.skipped.len()))
}

fn terminal_metadata_error(error: &DriverError) -> bool {
    matches!(
        error,
        DriverError::Cancelled
            | DriverError::TimedOut
            | DriverError::Disconnected
            | DriverError::ConnectionRefused
            | DriverError::AuthFailed
            | DriverError::Tls(_)
            | DriverError::PolicyDenied(_)
            | DriverError::OperationOutcomeUnknown { .. }
    )
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
