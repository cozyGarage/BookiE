use tablepro_core::{
    Connection, DriverCapabilities, DriverError, ForeignKeyInfo, IndexInfo, OperationControl, check_pre_dispatch,
};

#[derive(Debug, Default)]
pub struct StructureMetadata {
    pub indexes_supported: bool,
    pub foreign_keys_supported: bool,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
}

pub async fn load_structure_metadata(
    connection: &dyn Connection,
    schema: Option<&str>,
    table: &str,
    capabilities: DriverCapabilities,
    control: &OperationControl,
) -> Result<StructureMetadata, DriverError> {
    check_pre_dispatch(control)?;
    let indexes = if capabilities.supports_index_metadata {
        connection.fetch_indexes_controlled(schema, table, control).await?
    } else {
        Vec::new()
    };
    check_pre_dispatch(control)?;
    let foreign_keys = if capabilities.supports_foreign_key_metadata {
        connection.fetch_foreign_keys_controlled(schema, table, control).await?
    } else {
        Vec::new()
    };
    check_pre_dispatch(control)?;
    Ok(StructureMetadata {
        indexes_supported: capabilities.supports_index_metadata,
        foreign_keys_supported: capabilities.supports_foreign_key_metadata,
        indexes,
        foreign_keys,
    })
}

#[cfg(test)]
#[path = "structure_metadata_tests.rs"]
mod tests;
