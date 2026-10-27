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
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use tablepro_core::{
        ColumnInfo, Connection, DriverCapabilities, DriverError, ExecResult, OperationControl, QueryResult, TableInfo,
        Value,
    };
    use tokio::time::Instant;
    use tokio_util::sync::CancellationToken;

    use super::load_structure_metadata;

    #[derive(Default)]
    struct Calls {
        indexes: AtomicUsize,
        foreign_keys: AtomicUsize,
        fail_indexes: AtomicBool,
        cancel_after_indexes: AtomicBool,
        cancellation: CancellationToken,
    }

    struct MetadataConnection(Arc<Calls>);

    #[async_trait]
    impl Connection for MetadataConnection {
        async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
            Ok(Vec::new())
        }

        async fn fetch_columns(&self, _schema: Option<&str>, _table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
            Ok(Vec::new())
        }

        async fn fetch_rows(
            &self,
            _schema: Option<&str>,
            _table: &str,
            _offset: u64,
            _limit: u64,
        ) -> Result<QueryResult, DriverError> {
            Err(DriverError::Unsupported("unused".into()))
        }

        async fn query(&self, _sql: &str) -> Result<QueryResult, DriverError> {
            Err(DriverError::Unsupported("unused".into()))
        }

        async fn execute(&self, _sql: &str) -> Result<ExecResult, DriverError> {
            Err(DriverError::Unsupported("unused".into()))
        }

        async fn execute_params(&self, _sql: &str, _params: &[Value]) -> Result<ExecResult, DriverError> {
            Err(DriverError::Unsupported("unused".into()))
        }

        async fn execute_in_transaction(&self, _statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
            Err(DriverError::Unsupported("unused".into()))
        }

        async fn ping(&self) -> Result<(), DriverError> {
            Ok(())
        }

        async fn close(self: Box<Self>) -> Result<(), DriverError> {
            Ok(())
        }

        async fn fetch_indexes(
            &self,
            _schema: Option<&str>,
            _table: &str,
        ) -> Result<Vec<tablepro_core::IndexInfo>, DriverError> {
            self.0.indexes.fetch_add(1, Ordering::SeqCst);
            if self.0.cancel_after_indexes.load(Ordering::SeqCst) {
                self.0.cancellation.cancel();
            }
            if self.0.fail_indexes.load(Ordering::SeqCst) {
                return Err(DriverError::query("index metadata failed"));
            }
            Ok(Vec::new())
        }

        async fn fetch_foreign_keys(
            &self,
            _schema: Option<&str>,
            _table: &str,
        ) -> Result<Vec<tablepro_core::ForeignKeyInfo>, DriverError> {
            self.0.foreign_keys.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        }
    }

    fn control(calls: &Calls) -> OperationControl {
        OperationControl::new(calls.cancellation.clone(), None)
    }

    #[tokio::test]
    async fn unsupported_metadata_performs_no_catalog_reads() {
        let calls = Arc::new(Calls::default());
        let connection = MetadataConnection(Arc::clone(&calls));

        let metadata = load_structure_metadata(
            &connection,
            Some("public"),
            "items",
            DriverCapabilities::default(),
            &control(&calls),
        )
        .await
        .expect("unsupported metadata is an empty successful result");

        assert!(!metadata.indexes_supported);
        assert!(!metadata.foreign_keys_supported);
        assert!(metadata.indexes.is_empty());
        assert!(metadata.foreign_keys.is_empty());
        assert_eq!(calls.indexes.load(Ordering::SeqCst), 0);
        assert_eq!(calls.foreign_keys.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn supported_empty_metadata_keeps_both_support_flags() {
        let calls = Arc::new(Calls::default());
        let connection = MetadataConnection(Arc::clone(&calls));
        let capabilities = DriverCapabilities {
            supports_index_metadata: true,
            supports_foreign_key_metadata: true,
            ..DriverCapabilities::default()
        };

        let metadata = load_structure_metadata(&connection, None, "items", capabilities, &control(&calls))
            .await
            .expect("supported empty metadata is successful");

        assert!(metadata.indexes_supported);
        assert!(metadata.foreign_keys_supported);
        assert!(metadata.indexes.is_empty());
        assert!(metadata.foreign_keys.is_empty());
        assert_eq!(calls.indexes.load(Ordering::SeqCst), 1);
        assert_eq!(calls.foreign_keys.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn each_metadata_category_uses_its_own_capability() {
        for (indexes_supported, foreign_keys_supported) in [(true, false), (false, true)] {
            let calls = Arc::new(Calls::default());
            let connection = MetadataConnection(Arc::clone(&calls));
            let capabilities = DriverCapabilities {
                supports_index_metadata: indexes_supported,
                supports_foreign_key_metadata: foreign_keys_supported,
                ..DriverCapabilities::default()
            };

            let metadata = load_structure_metadata(&connection, None, "items", capabilities, &control(&calls))
                .await
                .expect("independent metadata categories load successfully");

            assert_eq!(metadata.indexes_supported, indexes_supported);
            assert_eq!(metadata.foreign_keys_supported, foreign_keys_supported);
            assert_eq!(calls.indexes.load(Ordering::SeqCst), usize::from(indexes_supported));
            assert_eq!(
                calls.foreign_keys.load(Ordering::SeqCst),
                usize::from(foreign_keys_supported)
            );
        }
    }

    #[tokio::test]
    async fn cancellation_between_reads_prevents_foreign_key_dispatch() {
        let calls = Arc::new(Calls::default());
        calls.cancel_after_indexes.store(true, Ordering::SeqCst);
        let connection = MetadataConnection(Arc::clone(&calls));
        let capabilities = DriverCapabilities {
            supports_index_metadata: true,
            supports_foreign_key_metadata: true,
            ..DriverCapabilities::default()
        };

        let result = load_structure_metadata(&connection, None, "items", capabilities, &control(&calls)).await;

        assert!(matches!(result, Err(DriverError::Cancelled)));
        assert_eq!(calls.indexes.load(Ordering::SeqCst), 1);
        assert_eq!(calls.foreign_keys.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn index_read_failure_does_not_start_foreign_key_read() {
        let calls = Arc::new(Calls::default());
        calls.fail_indexes.store(true, Ordering::SeqCst);
        let connection = MetadataConnection(Arc::clone(&calls));
        let capabilities = DriverCapabilities {
            supports_index_metadata: true,
            supports_foreign_key_metadata: true,
            ..DriverCapabilities::default()
        };

        let result = load_structure_metadata(&connection, None, "items", capabilities, &control(&calls)).await;

        assert!(matches!(result, Err(DriverError::Query { .. })));
        assert_eq!(calls.indexes.load(Ordering::SeqCst), 1);
        assert_eq!(calls.foreign_keys.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn expired_deadline_refuses_before_metadata_dispatch() {
        let calls = Arc::new(Calls::default());
        let connection = MetadataConnection(Arc::clone(&calls));
        let expired = OperationControl::new(
            CancellationToken::new(),
            Some(Instant::now() - std::time::Duration::from_secs(1)),
        );
        let capabilities = DriverCapabilities {
            supports_index_metadata: true,
            supports_foreign_key_metadata: true,
            ..DriverCapabilities::default()
        };

        let result = load_structure_metadata(&connection, None, "items", capabilities, &expired).await;

        assert!(matches!(result, Err(DriverError::TimedOut)));
        assert_eq!(calls.indexes.load(Ordering::SeqCst), 0);
        assert_eq!(calls.foreign_keys.load(Ordering::SeqCst), 0);
    }
}
