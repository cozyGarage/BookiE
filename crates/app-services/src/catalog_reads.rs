use bookie_core::{CatalogObject, CatalogObjectKind, Connection, DriverError, OperationControl, TableInfo};

pub async fn list_tables_excluding(
    connection: &dyn Connection,
    hidden_schemas: &[String],
    control: &OperationControl,
) -> Result<Vec<TableInfo>, DriverError> {
    connection
        .list_tables_excluding_controlled(hidden_schemas, control)
        .await
}

pub async fn list_views_excluding(
    connection: &dyn Connection,
    hidden_schemas: &[String],
    control: &OperationControl,
) -> Result<Vec<TableInfo>, DriverError> {
    connection
        .list_views_excluding_controlled(hidden_schemas, control)
        .await
}

pub async fn list_objects_excluding(
    connection: &dyn Connection,
    kind: CatalogObjectKind,
    schema: Option<&str>,
    hidden_schemas: &[String],
    control: &OperationControl,
) -> Result<Vec<CatalogObject>, DriverError> {
    connection
        .list_objects_excluding_controlled(kind, schema, hidden_schemas, control)
        .await
}

#[cfg(test)]
mod tests;
