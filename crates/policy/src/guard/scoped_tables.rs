use super::*;

impl PolicyGuard {
    pub(super) async fn governed_list_tables_in(
        &self,
        schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("LIST TABLES", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read("LIST TABLES", self.inner.list_tables_in(schemas, control))
            .await;
        let rows = result.as_ref().ok().map(|tables| tables.len() as u64);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }
}
