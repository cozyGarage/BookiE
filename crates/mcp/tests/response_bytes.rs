#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::Arc;

use async_trait::async_trait;
use bookie_core::{
    ColumnInfo, Connection, DriverError, Environment, ExecResult, OperationControl, QueryResult, QueryResultBatch,
    TableInfo, Transaction, Value,
};
use bookie_mcp::{ConnectionProvider, McpBridge, McpLimits, TokenPermissions, TokenStore, dispatch};
use bookie_policy::Principal;
use bookie_storage::SavedConnection;
use uuid::Uuid;

const ROW_BYTES: usize = 4 * 1024 * 1024;

struct RowsProvider {
    connection_id: Uuid,
    fixture: RowFixture,
}

#[async_trait]
impl ConnectionProvider for RowsProvider {
    async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
        Ok(vec![saved_connection(self.connection_id)])
    }

    async fn connection(&self, _: Uuid, _: Principal) -> Result<Arc<dyn Connection>, String> {
        Ok(Arc::new(RowsConnection { fixture: self.fixture }))
    }
}

#[derive(Clone, Copy)]
enum RowFixture {
    Big,
    Escaped,
}

impl RowFixture {
    fn result(self) -> QueryResult {
        match self {
            Self::Big => QueryResult {
                columns: Vec::new(),
                rows: (0..3)
                    .map(|_| vec![Value::Text("x".repeat(ROW_BYTES))])
                    .collect::<Vec<_>>()
                    .into(),
                truncated: false,
            },
            Self::Escaped => QueryResult {
                columns: Vec::new(),
                rows: (0..4)
                    .map(|_| vec![Value::Text("\0".repeat(10 * 1024)), Value::Bytes(vec![0; 10 * 1024])])
                    .collect::<Vec<_>>()
                    .into(),
                truncated: false,
            },
        }
    }
}

struct RowsConnection {
    fixture: RowFixture,
}

#[async_trait]
impl Connection for RowsConnection {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_columns(&self, _: Option<&str>, _: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_rows(&self, _: Option<&str>, _: &str, _: u64, _: u64) -> Result<QueryResult, DriverError> {
        Ok(self.fixture.result())
    }

    async fn query(&self, _: &str) -> Result<QueryResult, DriverError> {
        Ok(self.fixture.result())
    }

    async fn query_result_sets_controlled(
        &self,
        sql: &str,
        _: &[Value],
        _: &OperationControl,
    ) -> Result<QueryResultBatch, DriverError> {
        let mut batch = QueryResultBatch::single(self.fixture.result());
        if matches!(self.fixture, RowFixture::Escaped) && !sql.contains("WHERE id = 1") {
            batch.result_sets.push(self.fixture.result());
        }
        Ok(batch)
    }

    async fn execute(&self, _: &str) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 0 })
    }

    async fn execute_params(&self, _: &str, _: &[Value]) -> Result<ExecResult, DriverError> {
        Ok(ExecResult { rows_affected: 0 })
    }

    async fn execute_in_transaction(&self, _: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Ok(Vec::new())
    }

    async fn begin(&self) -> Result<Box<dyn Transaction>, DriverError> {
        Err(DriverError::Unsupported("begin".into()))
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

#[tokio::test]
async fn a_query_response_is_cut_at_the_byte_budget_not_only_at_the_row_limit() {
    let directory = tempfile::TempDir::new().unwrap();
    let connection_id = Uuid::new_v4();
    let tokens = Arc::new(TokenStore::open(directory.path().join("tokens.json")).unwrap());
    let (_, plaintext) = tokens
        .issue("bytes".into(), TokenPermissions::ReadOnly, vec![connection_id], None)
        .unwrap();
    let token = tokens.authenticate(&plaintext).unwrap();
    let bridge = McpBridge::new(
        Arc::new(RowsProvider {
            connection_id,
            fixture: RowFixture::Big,
        }),
        tokens,
    );

    let batch = bridge
        .execute_query(&token, connection_id, "SELECT note FROM notes")
        .await
        .unwrap();

    let kept = batch.result_sets[0].rows.len();
    assert!(kept < 3, "12 MiB of rows must not all be returned, kept {kept}");
    assert!(batch.truncated && batch.result_sets[0].truncated);

    let table = bridge
        .browse_table(&token, connection_id, None, "notes".into(), 0, 3)
        .await
        .unwrap();
    assert!(table.rows.len() < 3, "browsing a table is bounded the same way");
    assert!(table.truncated);
}

#[tokio::test]
async fn escaped_text_does_not_exceed_the_serialized_tool_response_budget() {
    const MAX_RESPONSE_BYTES: usize = 256 * 1024;
    let directory = tempfile::TempDir::new().unwrap();
    let connection_id = Uuid::new_v4();
    let tokens = Arc::new(TokenStore::open(directory.path().join("tokens.json")).unwrap());
    let (_, plaintext) = tokens
        .issue("bytes".into(), TokenPermissions::ReadOnly, vec![connection_id], None)
        .unwrap();
    let token = tokens.authenticate(&plaintext).unwrap();
    let bridge = McpBridge::with_limits(
        Arc::new(RowsProvider {
            connection_id,
            fixture: RowFixture::Escaped,
        }),
        tokens,
        McpLimits {
            max_response_bytes: MAX_RESPONSE_BYTES,
            ..McpLimits::default()
        },
    );

    for (tool, arguments) in [
        (
            "execute_query",
            serde_json::json!({"connection_id": connection_id, "sql": "SELECT note FROM notes"}),
        ),
        (
            "browse_table",
            serde_json::json!({"connection_id": connection_id, "table": "notes", "offset": 0, "limit": 4}),
        ),
        (
            "export_data",
            serde_json::json!({"connection_id": connection_id, "sql": "SELECT note FROM notes", "format": "json"}),
        ),
        (
            "export_data",
            serde_json::json!({"connection_id": connection_id, "sql": "SELECT note FROM notes WHERE id = 1", "format": "csv"}),
        ),
    ] {
        assert_tool_response_bounded(&bridge, &token, tool, arguments, MAX_RESPONSE_BYTES).await;
    }
}

async fn assert_tool_response_bounded(
    bridge: &McpBridge,
    token: &bookie_mcp::McpToken,
    tool: &str,
    arguments: serde_json::Value,
    limit: usize,
) {
    let response = dispatch(bridge, token, tool, arguments).await.unwrap();
    let bytes = serde_json::to_vec(&response).unwrap().len();
    assert!(
        bytes <= limit,
        "{tool} encoded {bytes} bytes above its {limit} byte cap"
    );
    assert_eq!(response["truncated"], true, "{tool}");
}

fn saved_connection(id: Uuid) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id,
        name: "timeout-bounds".into(),
        driver_id: "postgres".into(),
        host: "localhost".into(),
        port: 5432,
        socket_dir: None,
        socket_path: None,
        database: "timeout-bounds".into(),
        username: "timeout-bounds".into(),
        use_tls: false,
        tls_mode: None,
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: false,
        auth_mode: Default::default(),
        environment: Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        attached_databases: Vec::new(),
        tls_strict_encryption: false,
    }
}
