use super::*;
use axum::http::HeaderValue;
use bookie_core::{ColumnInfo, Connection, DriverError, Environment, ExecResult, QueryResult, TableInfo, Value};
use bookie_policy::{
    ApprovalOutcome, ApprovalRequest, ApprovalSink, AuditError, AuditEvent, AuditRecordPhase, AuditSink, AuditState,
    AuditTerminalStatus, GuardContext, PolicyConfig, PolicyGuard, Principal,
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::Notify,
};

fn headers_with_origin(origin: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
    headers
}

#[test]
fn query_tool_descriptions_explain_raw_json_text_encoding() {
    for name in ["execute_query", "export_data"] {
        let description = tool_description(name);
        assert!(description.contains("bookie-tagged-v1"));
        assert!(description.contains("raw JSON text remains exact under json_text"));
    }
}

struct NoProvider;

#[async_trait::async_trait]
impl crate::bridge::ConnectionProvider for NoProvider {
    async fn list_saved_connections(&self) -> Result<Vec<bookie_storage::SavedConnection>, String> {
        Ok(Vec::new())
    }
    async fn connection(
        &self,
        _connection_id: uuid::Uuid,
        _principal: bookie_policy::Principal,
    ) -> Result<Arc<dyn bookie_core::Connection>, String> {
        Err("no connections in this test".into())
    }
}

/// Keeps the temporary token-store directory alive for the test's
/// duration -- dropping it early deletes tokens.json out from under
/// a later `authenticate` call.
fn bridge_with_token() -> (tempfile::TempDir, McpBridge, String) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(crate::tokens::TokenStore::open(dir.path().join("tokens.json")).unwrap());
    let (_metadata, plaintext) = store
        .issue("test".into(), crate::TokenPermissions::ReadOnly, vec![], None)
        .unwrap();
    (dir, McpBridge::new(Arc::new(NoProvider), store), plaintext)
}

#[tokio::test]
async fn streamable_http_server_stops_after_shutdown_is_requested() {
    let (_dir, bridge, _) = bridge_with_token();
    let shutdown = tokio_util::sync::CancellationToken::new();
    let server_shutdown = shutdown.clone();
    let task = tokio::spawn(async move {
        serve_streamable_http_until(
            Arc::new(bridge),
            McpServerConfig {
                bind_host: "127.0.0.1".into(),
                bind_port: 0,
            },
            server_shutdown,
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    shutdown.cancel();
    let result = task.await.unwrap();
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn a_disconnected_http_caller_cancels_but_does_not_drop_its_operation() {
    let (response_tx, response_rx) = tokio::sync::oneshot::channel::<()>();
    let cancellation = tokio_util::sync::CancellationToken::new();
    let observed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let operation_cancellation = cancellation.clone();
    let operation_observed = observed.clone();
    let task = tokio::spawn(supervise_disconnect_aware(
        response_tx,
        cancellation.clone(),
        async move {
            operation_cancellation.cancelled().await;
            operation_observed.store(true, std::sync::atomic::Ordering::SeqCst);
        },
    ));

    drop(response_rx);
    task.await.expect("request supervisor should finish after cancellation");

    assert!(cancellation.is_cancelled());
    assert!(observed.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn a_completed_http_request_delivers_its_response_and_never_cancels() {
    let (response_tx, response_rx) = tokio::sync::oneshot::channel::<u8>();
    let cancellation = tokio_util::sync::CancellationToken::new();

    supervise_disconnect_aware(response_tx, cancellation.clone(), async { 7 }).await;

    assert_eq!(response_rx.await, Ok(7));
    assert!(!cancellation.is_cancelled());
}

#[tokio::test]
async fn an_http_body_over_the_request_limit_is_refused() {
    use tokio::io::AsyncReadExt;

    let dir = tempfile::tempdir().unwrap();
    let tokens = Arc::new(crate::tokens::TokenStore::open(dir.path().join("tokens.json")).unwrap());
    let router = http_router(Arc::new(McpBridge::new(Arc::new(NoProvider), tokens)));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await });

    let body = vec![b' '; MAX_REQUEST_BYTES + 1];
    let mut stream = TcpStream::connect(address).await.unwrap();
    let head = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await.unwrap();
    let _ = stream.write_all(&body).await;
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response).await;
    server.abort();

    let status = String::from_utf8_lossy(&response);
    assert!(
        status.starts_with("HTTP/1.1 413"),
        "{}",
        status.lines().next().unwrap_or_default()
    );
}

struct PendingApproval {
    requested: Arc<Notify>,
}

#[async_trait::async_trait]
impl ApprovalSink for PendingApproval {
    async fn request(&self, _request: ApprovalRequest) -> ApprovalOutcome {
        self.requested.notify_one();
        std::future::pending().await
    }
}

struct RecordingAudit {
    events: std::sync::Mutex<Vec<AuditEvent>>,
    recorded: Notify,
}

#[async_trait::async_trait]
impl AuditSink for RecordingAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.events.lock().expect("audit events").push(event);
        self.recorded.notify_one();
        Ok(())
    }
}

struct WriteProbeConnection {
    executions: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Connection for WriteProbeConnection {
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
        Ok(QueryResult {
            columns: Vec::new(),
            rows: Vec::new().into(),
            truncated: false,
        })
    }
    async fn query(&self, _sql: &str) -> Result<QueryResult, DriverError> {
        Ok(QueryResult {
            columns: Vec::new(),
            rows: Vec::new().into(),
            truncated: false,
        })
    }
    async fn execute(&self, _sql: &str) -> Result<ExecResult, DriverError> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        Ok(ExecResult { rows_affected: 1 })
    }
    async fn execute_params(&self, sql: &str, _params: &[Value]) -> Result<ExecResult, DriverError> {
        self.execute(sql).await
    }
    async fn execute_in_transaction(&self, _statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Ok(Vec::new())
    }
    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }
    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct ApprovalProvider {
    saved: bookie_storage::SavedConnection,
    connection: Arc<dyn Connection>,
}

#[async_trait::async_trait]
impl crate::bridge::ConnectionProvider for ApprovalProvider {
    async fn list_saved_connections(&self) -> Result<Vec<bookie_storage::SavedConnection>, String> {
        Ok(vec![self.saved.clone()])
    }
    async fn connection(
        &self,
        _connection_id: uuid::Uuid,
        _principal: Principal,
    ) -> Result<Arc<dyn Connection>, String> {
        Ok(self.connection.clone())
    }
}

struct ApprovalFixture {
    _directory: tempfile::TempDir,
    bridge: Arc<McpBridge>,
    connection_id: uuid::Uuid,
    plaintext: String,
    requested: Arc<Notify>,
    audit: Arc<RecordingAudit>,
    executions: Arc<AtomicUsize>,
}

fn approval_fixture() -> ApprovalFixture {
    let connection_id = uuid::Uuid::new_v4();
    let saved = serde_json::from_value(serde_json::json!({
        "id": connection_id,
        "name": "approval fixture",
        "driver_id": "sqlite",
        "host": "",
        "port": 0,
        "database": ":memory:",
        "username": "",
    }))
    .expect("saved connection fixture");
    let requested = Arc::new(Notify::new());
    let audit = Arc::new(RecordingAudit {
        events: Mutex::new(Vec::new()),
        recorded: Notify::new(),
    });
    let executions = Arc::new(AtomicUsize::new(0));
    let guarded: Arc<dyn Connection> = Arc::new(PolicyGuard::new(
        Arc::new(WriteProbeConnection {
            executions: executions.clone(),
        }),
        GuardContext {
            connection_id,
            connection_name: "approval fixture".into(),
            driver_id: "sqlite".into(),
            environment: Environment::Local,
            read_only: false,
            principal: Principal::Agent {
                token: "test-token".into(),
                client: Some("disconnect test".into()),
                model: None,
            },
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(PendingApproval {
                requested: requested.clone(),
            }),
            audit: audit.clone(),
            audit_state: Arc::new(AuditState::new()),
        },
    ));
    let directory = tempfile::tempdir().unwrap();
    let tokens = Arc::new(crate::tokens::TokenStore::open(directory.path().join("tokens.json")).unwrap());
    let (_, plaintext) = tokens
        .issue(
            "disconnect test".into(),
            crate::auth::TokenPermissions::ReadWrite,
            vec![connection_id],
            None,
        )
        .unwrap();
    let bridge = Arc::new(McpBridge::new(
        Arc::new(ApprovalProvider {
            saved,
            connection: guarded,
        }),
        tokens,
    ));
    ApprovalFixture {
        _directory: directory,
        bridge,
        connection_id,
        plaintext,
        requested,
        audit,
        executions,
    }
}

#[tokio::test]
async fn disconnecting_during_agent_approval_records_cancellation_and_never_executes() {
    let ApprovalFixture {
        _directory,
        bridge,
        connection_id,
        plaintext,
        requested,
        audit,
        executions,
    } = approval_fixture();

    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = probe.local_addr().unwrap();
    drop(probe);
    let shutdown = tokio_util::sync::CancellationToken::new();
    let server_shutdown = shutdown.clone();
    let server = tokio::spawn(async move {
        serve_streamable_http_until(
            bridge,
            McpServerConfig {
                bind_host: "127.0.0.1".into(),
                bind_port: address.port(),
            },
            server_shutdown,
        )
        .await
    });
    let mut stream = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match TcpStream::connect(address).await {
                Ok(stream) => break stream,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .expect("MCP listener should start");
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "execute_write",
            "arguments": {
                "connection_id": connection_id,
                "sql": "INSERT INTO jobs(id) VALUES (1)",
                "preview": false,
                "token": plaintext,
            }
        }
    })
    .to_string();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), requested.notified())
        .await
        .expect("write should reach human approval");
    drop(stream);

    tokio::time::timeout(std::time::Duration::from_secs(5), audit.recorded.notified())
        .await
        .expect("disconnect should produce a terminal audit record");
    {
        let events = audit.events.lock().expect("audit events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].phase, AuditRecordPhase::Outcome);
        assert_eq!(events[0].terminal_status, AuditTerminalStatus::Cancelled);
    }
    assert_eq!(
        executions.load(Ordering::SeqCst),
        0,
        "cancelled write must not reach the driver"
    );

    shutdown.cancel();
    assert!(server.await.expect("server task").is_ok());
}

struct StdioClient {
    input: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    output: BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    server: tokio::task::JoinHandle<Result<(), String>>,
}

fn start_stdio_session(bridge: Arc<McpBridge>) -> StdioClient {
    let (client, server) = tokio::io::duplex(1 << 16);
    let (server_read, server_write) = tokio::io::split(server);
    let (client_read, client_write) = tokio::io::split(client);
    let server =
        tokio::spawn(async move { serve_json_rpc_lines(bridge, BufReader::new(server_read), server_write).await });
    StdioClient {
        input: client_write,
        output: BufReader::new(client_read),
        server,
    }
}

impl StdioClient {
    async fn send(&mut self, message: JsonValue) {
        self.input.write_all(format!("{message}\n").as_bytes()).await.unwrap();
    }

    async fn next_message(&mut self, within: std::time::Duration) -> Option<JsonValue> {
        let mut line = String::new();
        let read = tokio::time::timeout(within, self.output.read_line(&mut line))
            .await
            .ok()?;
        (read.ok()? > 0).then(|| serde_json::from_str(line.trim()).unwrap())
    }

    async fn finish(mut self) {
        self.input.shutdown().await.unwrap();
        let ended = tokio::time::timeout(std::time::Duration::from_secs(5), self.server).await;
        assert!(
            matches!(ended, Ok(Ok(Ok(())))),
            "the session should end cleanly: {ended:?}"
        );
    }
}

fn pending_write(id: u64, fixture: &ApprovalFixture) -> JsonValue {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": "execute_write",
            "arguments": {
                "connection_id": fixture.connection_id,
                "sql": "INSERT INTO jobs(id) VALUES (1)",
                "preview": false,
                "token": fixture.plaintext,
            }
        }
    })
}

const SOON: std::time::Duration = std::time::Duration::from_secs(3);

async fn wait_for_approval_request(requested: &Notify) {
    tokio::time::timeout(SOON, requested.notified())
        .await
        .expect("the write should reach human approval");
}

#[tokio::test]
async fn a_ping_is_answered_while_a_tool_call_waits_for_approval() {
    let fixture = approval_fixture();
    let mut client = start_stdio_session(fixture.bridge.clone());
    client.send(pending_write(1, &fixture)).await;
    wait_for_approval_request(&fixture.requested).await;

    client.send(json!({"jsonrpc": "2.0", "id": 2, "method": "ping"})).await;

    let answer = client
        .next_message(SOON)
        .await
        .expect("the ping must not wait for the call");
    assert_eq!((answer["id"].clone(), answer["result"].clone()), (json!(2), json!({})));
    assert!(
        client
            .next_message(std::time::Duration::from_millis(150))
            .await
            .is_none()
    );
    client.finish().await;
    wait_for_cancelled_outcome(&fixture).await;
}

async fn wait_for_cancelled_outcome(fixture: &ApprovalFixture) {
    tokio::time::timeout(SOON, fixture.audit.recorded.notified())
        .await
        .expect("a cancelled call must reach a terminal audit state");
    let events = fixture.audit.events.lock().expect("audit events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].phase, AuditRecordPhase::Outcome);
    assert_eq!(events[0].terminal_status, AuditTerminalStatus::Cancelled);
    assert_eq!(
        fixture.executions.load(Ordering::SeqCst),
        0,
        "a cancelled write must not run"
    );
}

#[tokio::test]
async fn a_cancelled_notification_stops_the_call_records_it_and_sends_no_response() {
    let fixture = approval_fixture();
    let mut client = start_stdio_session(fixture.bridge.clone());
    client.send(pending_write(7, &fixture)).await;
    wait_for_approval_request(&fixture.requested).await;

    client
        .send(json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 7}}))
        .await;
    wait_for_cancelled_outcome(&fixture).await;

    client.send(json!({"jsonrpc": "2.0", "id": 8, "method": "ping"})).await;
    let next = client.next_message(SOON).await.expect("the ping after the cancel");
    assert_eq!(next["id"], 8, "the cancelled call must not be answered: {next}");
    client.finish().await;
}

#[tokio::test]
async fn closing_the_input_cancels_calls_that_are_still_waiting() {
    let fixture = approval_fixture();
    let mut client = start_stdio_session(fixture.bridge.clone());
    client.send(pending_write(1, &fixture)).await;
    wait_for_approval_request(&fixture.requested).await;

    client.finish().await;

    wait_for_cancelled_outcome(&fixture).await;
}

#[tokio::test]
async fn calls_beyond_the_concurrency_limit_are_refused_at_once() {
    let fixture = approval_fixture();
    let mut client = start_stdio_session(fixture.bridge.clone());
    for id in 1..=MAX_IN_FLIGHT_CALLS as u64 + 1 {
        client.send(pending_write(id, &fixture)).await;
    }

    let refusal = client.next_message(SOON).await.expect("the extra call must be refused");

    assert_eq!(refusal["id"], MAX_IN_FLIGHT_CALLS as u64 + 1);
    assert_eq!(refusal["error"]["code"], -32000, "{refusal}");
    client.finish().await;
}

#[tokio::test]
async fn a_second_call_with_the_id_of_one_in_flight_is_refused() {
    let fixture = approval_fixture();
    let mut client = start_stdio_session(fixture.bridge.clone());
    client.send(pending_write(1, &fixture)).await;
    wait_for_approval_request(&fixture.requested).await;

    client.send(pending_write(1, &fixture)).await;

    let refusal = client
        .next_message(SOON)
        .await
        .expect("the duplicate id must be refused");
    assert_eq!(refusal["error"]["code"], -32600, "{refusal}");
    client.finish().await;
}

/// H-adjacent finding: tools/list disclosed the full tool catalogue to
/// any caller that could reach the listener, with no token at all.
#[test]
fn tools_list_is_refused_without_a_token() {
    let (_dir, bridge, _) = bridge_with_token();
    let error = handle_tools_list(&bridge, &json!({})).unwrap_err();
    assert!(error.contains("token"), "{error}");
}

#[test]
fn tools_list_is_refused_with_a_bad_token() {
    let (_dir, bridge, _) = bridge_with_token();
    let error = handle_tools_list(&bridge, &json!({"token": "not-a-real-token"})).unwrap_err();
    assert!(!error.is_empty());
}

#[test]
fn tools_list_succeeds_with_a_valid_token() {
    let (_dir, bridge, token) = bridge_with_token();
    let result = handle_tools_list(&bridge, &json!({"token": token})).unwrap();
    let tools = result["tools"].as_array().unwrap();
    assert_eq!(tools.len(), TOOL_NAMES.len());
}

#[test]
fn tools_list_declares_conservative_safety_annotations() {
    let (_dir, bridge, token) = bridge_with_token();
    let result = handle_tools_list(&bridge, &json!({"token": token})).unwrap();
    let hints = |name: &str| {
        result["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == name)
            .map(|tool| tool["annotations"].clone())
            .unwrap()
    };
    assert_eq!(hints("list_tables")["readOnlyHint"], true);
    assert_eq!(hints("browse_table")["readOnlyHint"], true);
    for name in ["execute_query", "execute_write", "explain_query", "export_data"] {
        let tool = hints(name);
        assert_eq!(tool["readOnlyHint"], false, "{name}");
        assert_eq!(tool["destructiveHint"], true, "{name}");
        assert_eq!(tool["idempotentHint"], false, "{name}");
    }
    assert!(TOOL_NAMES.iter().all(|name| hints(name)["openWorldHint"] == false));
}

/// A minimal in-memory `AsyncWrite` sink so `serve_json_rpc_lines` can be
/// driven from a test without real stdio: pushes every write straight
/// into a shared buffer the test can inspect afterward.
#[derive(Clone)]
struct RecordingWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl tokio::io::AsyncWrite for RecordingWriter {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.0.lock().unwrap().extend_from_slice(buf);
        std::task::Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

/// The stdio server used to terminate the whole session on the first
/// non-UTF-8 byte instead of resyncing the way it already does for
/// invalid JSON. A malformed line must produce one parse-error
/// response and leave the loop able to answer the next request.
#[tokio::test]
async fn a_non_utf8_line_is_resynced_instead_of_ending_the_session() {
    let (_dir, bridge, _) = bridge_with_token();
    let mut input = Vec::new();
    input.extend_from_slice(&[0xFF, 0xFE, 0xFD, b'\n']);
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n");
    let reader = BufReader::new(std::io::Cursor::new(input));
    let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let writer = RecordingWriter(output.clone());

    serve_json_rpc_lines(Arc::new(bridge), reader, writer).await.unwrap();

    let bytes = output.lock().unwrap().clone();
    let text = String::from_utf8(bytes).unwrap();
    let mut responses = text
        .lines()
        .map(|line| serde_json::from_str::<JsonValue>(line).unwrap());
    let first = responses.next().unwrap();
    assert_eq!(first["error"]["code"], -32700, "{first}");
    let second = responses.next().unwrap();
    assert_eq!(
        second["result"],
        json!({}),
        "the ping after the bad line must still be answered: {second}"
    );
}

#[tokio::test]
async fn discarding_an_oversized_line_resyncs_on_the_next_request() {
    let oversized = "x".repeat(DISCARD_CHUNK_BYTES as usize * 3);
    let payload = format!("{oversized}\n{{\"id\":1}}\n");
    let mut reader = BufReader::new(std::io::Cursor::new(payload.into_bytes()));

    discard_rest_of_line(&mut reader).await.unwrap();

    let mut next = String::new();
    reader.read_line(&mut next).await.unwrap();
    assert_eq!(next.trim(), "{\"id\":1}");
}

#[tokio::test]
async fn discarding_stops_at_end_of_input() {
    let mut reader = BufReader::new(std::io::Cursor::new(b"no newline here".to_vec()));
    discard_rest_of_line(&mut reader).await.unwrap();
    let mut next = String::new();
    assert_eq!(reader.read_line(&mut next).await.unwrap(), 0);
}

#[test]
fn the_advertised_protocol_revision_is_the_current_one() {
    let result = initialize_result(&json!({})).expect("no requested revision negotiates");
    assert_eq!(result["protocolVersion"], "2026-07-28");
    assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn an_older_client_revision_is_accepted_and_echoed() {
    for older in ["2025-11-25", "2025-06-18"] {
        let result = initialize_result(&json!({"protocolVersion": older})).expect("older revision");
        assert_eq!(result["protocolVersion"], older);
    }
}

#[test]
fn an_unknown_protocol_revision_is_refused_as_invalid_params() {
    for bad in [json!("2019-01-01"), json!("garbage"), json!(7), json!(null)] {
        let message =
            initialize_result(&json!({"protocolVersion": bad})).expect_err("an unknown revision must be refused");
        let error = json_rpc_error(&JsonValue::from(1), -32602, message);
        assert_eq!(error["error"]["code"], -32602);
    }
}

#[test]
fn loopback_hosts_resolve_to_a_loopback_socket() {
    for host in ["127.0.0.1", "localhost", "::1", "[::1]", "127.0.0.53", " 127.0.0.1 "] {
        let addr = loopback_bind_addr(host, 17432).unwrap_or_else(|e| panic!("{host}: {e}"));
        assert!(addr.ip().is_loopback(), "{host}");
        assert_eq!(addr.port(), 17432);
    }
}

#[test]
fn a_non_loopback_bind_host_is_refused() {
    for host in [
        "0.0.0.0",
        "::",
        "[::]",
        "192.168.1.10",
        "10.0.0.1",
        "203.0.113.5",
        "example.com",
        "127.0.0.1.evil.com",
        "",
    ] {
        let error = loopback_bind_addr(host, 17432).expect_err(host);
        assert!(error.contains("refusing to bind"), "{host}: {error}");
    }
}

#[test]
fn native_requests_without_origin_are_allowed() {
    assert!(origin_is_allowed(&HeaderMap::new()));
}

#[test]
fn trusted_browser_origins_are_allowed() {
    for origin in [
        "http://localhost:3000",
        "http://127.0.0.1:8080",
        "http://[::1]:8080",
        "https://claude.ai",
        "https://app.cursor.com",
    ] {
        assert!(origin_is_allowed(&headers_with_origin(origin)), "{origin}");
    }
}

#[test]
fn untrusted_browser_origins_are_rejected() {
    for origin in [
        "https://example.com",
        "https://cursor.com",
        "https://evil.claude.ai",
        "file:///tmp/request",
        "null",
    ] {
        assert!(!origin_is_allowed(&headers_with_origin(origin)), "{origin}");
    }
}
