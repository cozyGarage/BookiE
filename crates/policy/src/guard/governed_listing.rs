use super::*;

impl PolicyGuard {
    pub(super) async fn governed_list_collations(&self) -> Result<Vec<bookie_core::CollationInfo>, DriverError> {
        let operation = self.metadata_operation("LIST COLLATIONS", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self.caught_read("LIST COLLATIONS", self.inner.list_collations()).await;
        let rows = result.as_ref().ok().map(|collations| collations.len() as u64);
        self.audit_read_result(&operation, start, &result, rows).await?;
        result
    }

    pub(super) async fn governed_list_collations_controlled(
        &self,
        control: &OperationControl,
    ) -> Result<Vec<bookie_core::CollationInfo>, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("LIST COLLATIONS", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read("LIST COLLATIONS", self.inner.list_collations_controlled(control))
            .await;
        let rows = result.as_ref().ok().map(|collations| collations.len() as u64);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }

    pub(super) async fn governed_table_sizes(
        &self,
        schema: Option<&str>,
    ) -> Result<Vec<bookie_core::TableSize>, DriverError> {
        let operation = self.metadata_operation("TABLE SIZES", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self.caught_read("TABLE SIZES", self.inner.table_sizes(schema)).await;
        let rows = result.as_ref().ok().map(|sizes| sizes.len() as u64);
        self.audit_read_result(&operation, start, &result, rows).await?;
        result
    }

    pub(super) async fn governed_table_sizes_controlled(
        &self,
        schema: Option<&str>,
        control: &OperationControl,
    ) -> Result<Vec<bookie_core::TableSize>, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("TABLE SIZES", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read("TABLE SIZES", self.inner.table_sizes_controlled(schema, control))
            .await;
        let rows = result.as_ref().ok().map(|sizes| sizes.len() as u64);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }

    pub(super) async fn governed_list_databases(&self) -> Result<Vec<String>, DriverError> {
        let operation = self.metadata_operation("LIST DATABASES", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self.caught_read("LIST DATABASES", self.inner.list_databases()).await;
        let rows = result.as_ref().ok().map(|databases| databases.len() as u64);
        self.audit_read_result(&operation, start, &result, rows).await?;
        result
    }

    pub(super) async fn governed_list_databases_controlled(
        &self,
        control: &OperationControl,
    ) -> Result<Vec<String>, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("LIST DATABASES", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read("LIST DATABASES", self.inner.list_databases_controlled(control))
            .await;
        let rows = result.as_ref().ok().map(|databases| databases.len() as u64);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }
}
