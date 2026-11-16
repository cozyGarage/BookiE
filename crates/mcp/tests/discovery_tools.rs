#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use bookie_core::{
    ColumnInfo, Connection, DriverError, Environment, QueryResult, RoutineDefinition, RoutineLookup, RoutineRef,
    TableInfo,
};
use bookie_mcp::{ConnectionProvider, McpBridge, McpToken, TokenPermissions, TokenStore};
use bookie_policy::{
    AuditError, AuditEvent, AuditSink, AuditState, AuditTerminalStatus, AutoApproveSink, GuardContext, PolicyConfig,
    PolicyGuard, Principal,
};
use bookie_storage::SavedConnection;
use serde_json::{Value as JsonValue, json};
use uuid::Uuid;

const DEFINITION: &str = "CREATE FUNCTION known() RETURNS int AS 'SELECT definition_marker'";

struct Fixture {
    tables: usize,
    columns: usize,
}

struct StubConn {
    fixture: Arc<Fixture>,
}

fn column(name: String) -> ColumnInfo {
    ColumnInfo {
        name,
        data_type: "integer".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

#[async_trait]
impl Connection for StubConn {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok((0..self.fixture.tables)
            .map(|index| TableInfo {
                schema: Some("public".into()),
                name: format!("t{index:04}"),
            })
            .collect())
    }

    async fn routine_definition(&self, routine: &RoutineRef) -> Result<RoutineLookup, DriverError> {
        Ok(match (routine.name.as_str(), routine.identity_arguments.as_deref()) {
            ("known", _) | ("over", Some("a integer")) => RoutineLookup::Found(RoutineDefinition {
                schema: routine.schema.clone(),
                name: routine.name.clone(),
                kind: "function".into(),
                identity_arguments: String::new(),
                definition: DEFINITION.into(),
                total_bytes: Some(DEFINITION.len() as u64),
                truncated: false,
            }),
            ("over", None) => RoutineLookup::Ambiguous {
                signatures: vec!["a integer".into(), "a text".into()],
            },
            _ => RoutineLookup::NotFound,
        })
    }

    async fn fetch_columns(&self, _: Option<&str>, _: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        Ok((0..self.fixture.columns)
            .map(|index| column(format!("c{index}")))
            .collect())
    }

    async fn fetch_rows(&self, _: Option<&str>, _: &str, _: u64, _: u64) -> Result<QueryResult, DriverError> {
        Err(DriverError::Unsupported("rows".into()))
    }

    async fn query(&self, _: &str) -> Result<QueryResult, DriverError> {
        Err(DriverError::Unsupported("query".into()))
    }

    async fn execute(&self, _: &str) -> Result<bookie_core::ExecResult, DriverError> {
        Err(DriverError::Unsupported("execute".into()))
    }

    async fn execute_params(&self, _: &str, _: &[bookie_core::Value]) -> Result<bookie_core::ExecResult, DriverError> {
        Err(DriverError::Unsupported("execute".into()))
    }

    async fn execute_in_transaction(&self, _: &[(String, Vec<bookie_core::Value>)]) -> Result<Vec<u64>, DriverError> {
        Err(DriverError::Unsupported("execute".into()))
    }

    async fn fetch_indexes(&self, _: Option<&str>, _: &str) -> Result<Vec<bookie_core::IndexInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_foreign_keys(
        &self,
        _: Option<&str>,
        _: &str,
    ) -> Result<Vec<bookie_core::ForeignKeyInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

#[derive(Default)]
struct RecordingAudit(Mutex<Vec<AuditEvent>>);

#[async_trait]
impl AuditSink for RecordingAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.0.lock().unwrap().push(event);
        Ok(())
    }
}

struct Provider {
    fixture: Arc<Fixture>,
    audit: Arc<RecordingAudit>,
    audit_state: Arc<AuditState>,
    connection_calls: AtomicUsize,
}

fn saved(id: Uuid) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id,
        name: "stub".into(),
        driver_id: "postgres".into(),
        host: "localhost".into(),
        port: 5432,
        socket_dir: None,
        socket_path: None,
        database: "stub".into(),
        username: "stub".into(),
        use_tls: false,
        tls_mode: None,
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: false,
        auth_mode: Default::default(),
        environment: Default::default(),
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        attached_databases: Vec::new(),
        tls_strict_encryption: false,
    }
}

#[async_trait]
impl ConnectionProvider for Provider {
    async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
        Ok(vec![saved(Uuid::nil()), saved(Uuid::max())])
    }

    async fn connection(&self, connection_id: Uuid, principal: Principal) -> Result<Arc<dyn Connection>, String> {
        self.connection_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(PolicyGuard::new(
            Arc::new(StubConn {
                fixture: self.fixture.clone(),
            }),
            GuardContext {
                connection_id,
                connection_name: "stub".into(),
                driver_id: "postgres".into(),
                environment: Environment::Local,
                read_only: false,
                principal,
                policy: Arc::new(PolicyConfig::default()),
                approval: Arc::new(AutoApproveSink),
                audit: self.audit.clone(),
                audit_state: self.audit_state.clone(),
            },
        )))
    }
}

struct Harness {
    provider: Arc<Provider>,
    bridge: McpBridge,
    token: McpToken,
    _dir: tempfile::TempDir,
}

fn harness(
    permissions: TokenPermissions,
    connections: Vec<Uuid>,
    tools: Option<Vec<String>>,
    fixture: Fixture,
) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(TokenStore::open(dir.path().join("tokens.json")).unwrap());
    let (_meta, plaintext) = store
        .issue_with_tools("agent".into(), permissions, connections, tools, None)
        .unwrap();
    let provider = Arc::new(Provider {
        fixture: Arc::new(fixture),
        audit: Arc::new(RecordingAudit::default()),
        audit_state: Arc::new(AuditState::new()),
        connection_calls: AtomicUsize::new(0),
    });
    let bridge = McpBridge::new(provider.clone(), store);
    let token = bridge.authenticate(&plaintext).unwrap();
    Harness {
        provider,
        bridge,
        token,
        _dir: dir,
    }
}

fn small() -> Fixture {
    Fixture { tables: 2, columns: 3 }
}

fn routine_args(connection: Uuid, name: &str) -> JsonValue {
    json!({"connection_id": connection.to_string(), "schema": "public", "name": name})
}

impl Harness {
    async fn call(&self, tool: &str, args: JsonValue) -> Result<JsonValue, String> {
        bookie_mcp::dispatch(&self.bridge, &self.token, tool, args).await
    }

    fn connections_opened(&self) -> usize {
        self.provider.connection_calls.load(Ordering::SeqCst)
    }
}

#[tokio::test]
async fn routine_source_returns_a_definition_through_the_guard_for_an_allowed_token() {
    let h = harness(TokenPermissions::ReadOnly, vec![Uuid::nil()], None, small());

    let found = h
        .call("routine_source", routine_args(Uuid::nil(), "known"))
        .await
        .unwrap();

    assert_eq!(found["found"], true);
    assert_eq!(found["definition"], DEFINITION);
    assert_eq!(found["truncated"], false);
    assert_eq!(h.connections_opened(), 1);
    let missing = h
        .call("routine_source", routine_args(Uuid::nil(), "hidden"))
        .await
        .unwrap();
    assert_eq!(missing, json!({"found": false}));
}

#[tokio::test]
async fn an_overloaded_routine_needs_its_signature_and_lists_the_candidates() {
    let h = harness(TokenPermissions::ReadOnly, vec![Uuid::nil()], None, small());

    let ambiguous = h
        .call("routine_source", routine_args(Uuid::nil(), "over"))
        .await
        .unwrap();
    assert_eq!(ambiguous["ambiguous"], true);
    assert_eq!(ambiguous["signatures"], json!(["a integer", "a text"]));

    let mut args = routine_args(Uuid::nil(), "over");
    args["arguments"] = json!("a integer");
    assert_eq!(h.call("routine_source", args).await.unwrap()["found"], true);
}

#[tokio::test]
async fn the_audit_journal_records_the_routine_read_and_never_its_text() {
    let h = harness(TokenPermissions::ReadOnly, vec![Uuid::nil()], None, small());
    h.call("routine_source", routine_args(Uuid::nil(), "known"))
        .await
        .unwrap();
    h.call("routine_source", routine_args(Uuid::nil(), "hidden"))
        .await
        .unwrap();

    let events = h.provider.audit.0.lock().unwrap();

    assert_eq!(events.len(), 4, "an intent and a terminal outcome per call");
    let outcomes: Vec<_> = events.iter().skip(1).step_by(2).collect();
    assert!(
        outcomes
            .iter()
            .all(|event| event.terminal_status == AuditTerminalStatus::Succeeded)
    );
    assert_eq!(outcomes[0].rows_affected, Some(1));
    assert_eq!(outcomes[1].rows_affected, Some(0));
    assert!(!format!("{events:?}").contains("definition_marker"));
}

#[tokio::test]
async fn discovery_tools_are_denied_for_a_connection_outside_the_allowlist_before_any_connection_opens() {
    let h = harness(TokenPermissions::ReadOnly, vec![Uuid::nil()], None, small());
    for tool in ["routine_source", "schema_context"] {
        let error = h.call(tool, routine_args(Uuid::max(), "known")).await.unwrap_err();
        assert!(
            error.contains("not allowed to access this connection"),
            "{tool}: {error}"
        );
    }
    let empty = harness(TokenPermissions::ReadOnly, vec![], None, small());
    for tool in ["routine_source", "schema_context"] {
        let error = empty.call(tool, routine_args(Uuid::nil(), "known")).await.unwrap_err();
        assert!(error.contains("allowlist"), "{tool}: {error}");
        assert_eq!(empty.connections_opened(), 0);
    }
    assert_eq!(h.connections_opened(), 0);
}

#[tokio::test]
async fn a_tool_allowlist_denies_every_other_tool_before_any_database_work() {
    let h = harness(
        TokenPermissions::ReadWrite,
        vec![Uuid::nil()],
        Some(vec!["schema_context".into()]),
        small(),
    );
    let id = Uuid::nil().to_string();

    for (tool, args) in [
        ("routine_source", routine_args(Uuid::nil(), "known")),
        ("list_tables", json!({"connection_id": id})),
        ("execute_query", json!({"connection_id": id, "sql": "SELECT 1"})),
        (
            "execute_write",
            json!({"connection_id": id, "sql": "DELETE FROM t", "preview": false}),
        ),
        ("list_connections", json!({})),
    ] {
        let error = h.call(tool, args).await.unwrap_err();
        assert!(error.contains("not allowed to call this tool"), "{tool}: {error}");
    }
    assert_eq!(h.connections_opened(), 0);
    assert!(h.provider.audit.0.lock().unwrap().is_empty());

    let allowed = h.call("schema_context", json!({"connection_id": id})).await.unwrap();
    assert_eq!(allowed["included_tables"], 2);
}

#[tokio::test]
async fn a_token_without_a_tool_allowlist_reaches_every_read_tool_its_scope_allows() {
    let h = harness(TokenPermissions::ReadOnly, vec![Uuid::nil()], None, small());
    let id = Uuid::nil().to_string();
    assert!(h.call("schema_context", json!({"connection_id": id})).await.is_ok());
    assert!(
        h.call("routine_source", routine_args(Uuid::nil(), "known"))
            .await
            .is_ok()
    );
    assert!(h.call("list_tables", json!({"connection_id": id})).await.is_ok());
    let write = h
        .call(
            "execute_write",
            json!({"connection_id": id, "sql": "DELETE FROM t", "preview": false}),
        )
        .await
        .unwrap_err();
    assert!(write.contains("scope"), "{write}");
}

#[tokio::test]
async fn an_unknown_or_missing_bearer_token_reaches_no_connection() {
    let h = harness(TokenPermissions::ReadOnly, vec![Uuid::nil()], None, small());
    assert!(h.bridge.authenticate("not-a-token").is_err());
    assert_eq!(h.connections_opened(), 0);
}

#[tokio::test]
async fn hostile_routine_arguments_are_refused_before_a_connection_opens() {
    let h = harness(TokenPermissions::ReadOnly, vec![Uuid::nil()], None, small());
    let id = Uuid::nil().to_string();
    for args in [
        json!({"connection_id": id, "schema": "public"}),
        json!({"connection_id": id, "schema": "a\u{0}b", "name": "f"}),
        json!({"connection_id": id, "schema": "public", "name": "f", "arguments": "x\u{7}"}),
        json!({"connection_id": id, "schema": "public", "name": "f", "arguments": 7}),
    ] {
        assert!(h.call("routine_source", args).await.is_err());
    }
    assert_eq!(h.connections_opened(), 0);
}

#[tokio::test]
async fn schema_context_is_bounded_and_says_which_cap_applied() {
    let h = harness(
        TokenPermissions::ReadOnly,
        vec![Uuid::nil()],
        None,
        Fixture {
            tables: 250,
            columns: 130,
        },
    );

    let context = h
        .call("schema_context", json!({"connection_id": Uuid::nil().to_string()}))
        .await
        .unwrap();

    assert_eq!(context["table_count"], 250);
    assert_eq!(context["truncated"], true);
    let tables = context["tables"].as_array().unwrap();
    assert!(tables.len() <= context["limits"]["max_tables"].as_u64().unwrap() as usize);
    assert_eq!(tables[0]["columns"].as_array().unwrap().len(), 100);
    assert_eq!(tables[0]["columns_truncated"], true);
    let columns: usize = tables
        .iter()
        .map(|table| table["columns"].as_array().unwrap().len())
        .sum();
    assert!(columns <= context["limits"]["max_columns"].as_u64().unwrap() as usize);
    assert!(context.to_string().len() < 2 * context["limits"]["max_bytes"].as_u64().unwrap() as usize);
    assert!(["tables", "columns", "bytes"].contains(&context["limit"].as_str().unwrap()));
}

#[tokio::test]
async fn schema_context_filters_to_one_schema() {
    let h = harness(TokenPermissions::ReadOnly, vec![Uuid::nil()], None, small());
    let id = Uuid::nil().to_string();

    let other = h
        .call("schema_context", json!({"connection_id": id, "schema": "elsewhere"}))
        .await
        .unwrap();
    assert_eq!(other["included_tables"], 0);
    assert_eq!(other["truncated"], false);
    let public = h
        .call("schema_context", json!({"connection_id": id, "schema": "public"}))
        .await
        .unwrap();
    assert_eq!(public["included_tables"], 2);
    assert_eq!(public["tables"][0]["columns"][0]["name"], "c0");
}

#[tokio::test]
async fn a_token_file_written_before_tool_allowlists_still_authenticates_and_reaches_the_new_tools() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tokens.json");
    let legacy = json!([{
        "id": Uuid::new_v4(),
        "name": "old",
        "token_hash": "6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b",
        "permissions": "read_only",
        "connection_allowlist": [Uuid::nil()],
        "created_at": "2026-01-01T00:00:00Z"
    }]);
    std::fs::write(&path, legacy.to_string()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let provider = Arc::new(Provider {
        fixture: Arc::new(small()),
        audit: Arc::new(RecordingAudit::default()),
        audit_state: Arc::new(AuditState::new()),
        connection_calls: AtomicUsize::new(0),
    });
    let bridge = McpBridge::new(provider, Arc::new(TokenStore::open(path).unwrap()));

    let token = bridge.authenticate("1").unwrap();
    let found = bookie_mcp::dispatch(&bridge, &token, "routine_source", routine_args(Uuid::nil(), "known"))
        .await
        .unwrap();

    assert_eq!(found["found"], true);
}
