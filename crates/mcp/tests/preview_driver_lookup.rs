#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tablepro_core::{
    ColumnInfo, Connection, DriverError, ExecResult, ForeignKeyInfo, IndexInfo, QueryResult, TableInfo, Transaction,
    Value,
};
use tablepro_mcp::{ConnectionProvider, McpBridge, TokenPermissions, TokenStore};
use tablepro_policy::Principal;
use tablepro_storage::SavedConnection;
use uuid::Uuid;

struct CountingTransaction {
    executed: Arc<AtomicUsize>,
}

#[async_trait]
impl Transaction for CountingTransaction {
    async fn query(&mut self, _: &str) -> Result<QueryResult, DriverError> {
        Ok(QueryResult {
            columns: Vec::new(),
            rows: vec![vec![Value::Int(0)]],
            truncated: false,
        })
    }

    async fn execute(&mut self, _: &str) -> Result<ExecResult, DriverError> {
        self.executed.fetch_add(1, Ordering::SeqCst);
        Ok(ExecResult { rows_affected: 1 })
    }

    async fn commit(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct TransactionalConnection {
    executed: Arc<AtomicUsize>,
}

#[async_trait]
impl Connection for TransactionalConnection {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_columns(&self, _: Option<&str>, _: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_rows(&self, _: Option<&str>, _: &str, _: u64, _: u64) -> Result<QueryResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn query(&self, _: &str) -> Result<QueryResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn execute(&self, _: &str) -> Result<ExecResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn execute_params(&self, _: &str, _: &[Value]) -> Result<ExecResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn execute_in_transaction(&self, _: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn fetch_indexes(&self, _: Option<&str>, _: &str) -> Result<Vec<IndexInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_foreign_keys(&self, _: Option<&str>, _: &str) -> Result<Vec<ForeignKeyInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn begin(&self) -> Result<Box<dyn Transaction>, DriverError> {
        Ok(Box::new(CountingTransaction {
            executed: self.executed.clone(),
        }))
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct UnreadableCatalogProvider {
    connection: Arc<dyn Connection>,
}

#[async_trait]
impl ConnectionProvider for UnreadableCatalogProvider {
    async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
        Err("the saved connection list could not be read".into())
    }

    async fn connection(&self, _: Uuid, _: Principal) -> Result<Arc<dyn Connection>, String> {
        Ok(self.connection.clone())
    }
}

#[tokio::test]
async fn a_preview_does_not_run_when_the_engine_cannot_be_identified() {
    let executed = Arc::new(AtomicUsize::new(0));
    let connection_id = Uuid::new_v4();
    let directory = tempfile::tempdir().unwrap();
    let tokens = Arc::new(TokenStore::open(directory.path().join("tokens.json")).unwrap());
    let (_metadata, plaintext) = tokens
        .issue("preview".into(), TokenPermissions::ReadWrite, vec![connection_id], None)
        .unwrap();
    let bridge = McpBridge::new(
        Arc::new(UnreadableCatalogProvider {
            connection: Arc::new(TransactionalConnection {
                executed: executed.clone(),
            }),
        }),
        tokens,
    );
    let token = bridge.authenticate(&plaintext).unwrap();

    let outcome = bridge
        .execute_write(&token, connection_id, "UPDATE legacy SET v = 2 WHERE id = 1", true)
        .await;

    assert!(outcome.is_err(), "{outcome:?}");
    assert_eq!(executed.load(Ordering::SeqCst), 0);
}
