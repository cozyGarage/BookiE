use super::*;

impl PolicyGuard {
    pub(super) async fn governed_list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        let operation = self.metadata_operation("LIST TABLES", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self.caught_read("LIST TABLES", self.inner.list_tables()).await;
        let rows = result.as_ref().ok().map(|tables| tables.len() as u64);
        self.audit_read_result(&operation, start, &result, rows).await?;
        result
    }

    pub(super) async fn governed_list_tables_controlled(
        &self,
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("LIST TABLES", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read("LIST TABLES", self.inner.list_tables_controlled(control))
            .await;
        let rows = result.as_ref().ok().map(|tables| tables.len() as u64);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }

    pub(super) async fn governed_list_views(&self) -> Result<Vec<TableInfo>, DriverError> {
        let operation = self.metadata_operation("LIST VIEWS", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self.caught_read("LIST VIEWS", self.inner.list_views()).await;
        let rows = result.as_ref().ok().map(|views| views.len() as u64);
        self.audit_read_result(&operation, start, &result, rows).await?;
        result
    }

    pub(super) async fn governed_list_views_controlled(
        &self,
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("LIST VIEWS", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read("LIST VIEWS", self.inner.list_views_controlled(control))
            .await;
        let rows = result.as_ref().ok().map(|views| views.len() as u64);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }

    pub(super) async fn governed_list_objects(
        &self,
        kind: bookie_core::CatalogObjectKind,
        schema: Option<&str>,
    ) -> Result<Vec<bookie_core::CatalogObject>, DriverError> {
        let operation = self.metadata_operation("LIST OBJECTS", catalog_target(kind, schema));
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read("LIST OBJECTS", self.inner.list_objects(kind, schema))
            .await;
        let rows = result.as_ref().ok().map(|objects| objects.len() as u64);
        self.audit_read_result(&operation, start, &result, rows).await?;
        result
    }

    pub(super) async fn governed_list_objects_controlled(
        &self,
        kind: bookie_core::CatalogObjectKind,
        schema: Option<&str>,
        control: &OperationControl,
    ) -> Result<Vec<bookie_core::CatalogObject>, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("LIST OBJECTS", catalog_target(kind, schema));
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read(
                "LIST OBJECTS",
                self.inner.list_objects_controlled(kind, schema, control),
            )
            .await;
        let rows = result.as_ref().ok().map(|objects| objects.len() as u64);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }

    pub(super) async fn governed_list_tables_excluding(
        &self,
        schemas: &[String],
    ) -> Result<Vec<TableInfo>, DriverError> {
        let operation = self.metadata_operation("LIST TABLES", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read("LIST TABLES", self.inner.list_tables_excluding(schemas))
            .await;
        let rows = result.as_ref().ok().map(|items| items.len() as u64);
        self.audit_read_result(&operation, start, &result, rows).await?;
        result
    }

    pub(super) async fn governed_list_tables_excluding_controlled(
        &self,
        schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("LIST TABLES", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read(
                "LIST TABLES",
                self.inner.list_tables_excluding_controlled(schemas, control),
            )
            .await;
        let rows = result.as_ref().ok().map(|items| items.len() as u64);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }

    pub(super) async fn governed_list_views_excluding(
        &self,
        schemas: &[String],
    ) -> Result<Vec<TableInfo>, DriverError> {
        let operation = self.metadata_operation("LIST VIEWS", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read("LIST VIEWS", self.inner.list_views_excluding(schemas))
            .await;
        let rows = result.as_ref().ok().map(|items| items.len() as u64);
        self.audit_read_result(&operation, start, &result, rows).await?;
        result
    }

    pub(super) async fn governed_list_views_excluding_controlled(
        &self,
        schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("LIST VIEWS", Vec::new());
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read(
                "LIST VIEWS",
                self.inner.list_views_excluding_controlled(schemas, control),
            )
            .await;
        let rows = result.as_ref().ok().map(|items| items.len() as u64);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }

    pub(super) async fn governed_list_objects_excluding(
        &self,
        kind: bookie_core::CatalogObjectKind,
        schema: Option<&str>,
        excluded_schemas: &[String],
    ) -> Result<Vec<bookie_core::CatalogObject>, DriverError> {
        let operation = self.metadata_operation("LIST OBJECTS", catalog_target(kind, schema));
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read(
                "LIST OBJECTS",
                self.inner.list_objects_excluding(kind, schema, excluded_schemas),
            )
            .await;
        let rows = result.as_ref().ok().map(|items| items.len() as u64);
        self.audit_read_result(&operation, start, &result, rows).await?;
        result
    }

    pub(super) async fn governed_list_objects_excluding_controlled(
        &self,
        kind: bookie_core::CatalogObjectKind,
        schema: Option<&str>,
        excluded_schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<bookie_core::CatalogObject>, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("LIST OBJECTS", catalog_target(kind, schema));
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read(
                "LIST OBJECTS",
                self.inner
                    .list_objects_excluding_controlled(kind, schema, excluded_schemas, control),
            )
            .await;
        let rows = result.as_ref().ok().map(|items| items.len() as u64);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }
}
