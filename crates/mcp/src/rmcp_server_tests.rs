use super::*;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct NoProvider;

#[async_trait::async_trait]
impl crate::bridge::ConnectionProvider for NoProvider {
    async fn list_saved_connections(&self) -> Result<Vec<tablepro_storage::SavedConnection>, String> {
        Ok(Vec::new())
    }
    async fn connection(
        &self,
        _connection_id: uuid::Uuid,
        _principal: tablepro_policy::Principal,
    ) -> Result<Arc<dyn tablepro_core::Connection>, String> {
        Err("no connections in this test".into())
    }
}

struct Client {
    writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    lines: tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
    next_id: u64,
    _dir: tempfile::TempDir,
    pub token: String,
}

impl Client {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::tokens::TokenStore::open(dir.path().join("tokens.json")).unwrap());
        let (_metadata, token) = store
            .issue("test".into(), crate::TokenPermissions::ReadOnly, vec![], None)
            .unwrap();
        let bridge = Arc::new(McpBridge::new(Arc::new(NoProvider), store));
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            let running = BridgeHandler::new(bridge).serve(server_io).await.unwrap();
            let _ = running.waiting().await;
        });
        let (read, writer) = tokio::io::split(client_io);
        let mut client = Self {
            writer,
            lines: BufReader::new(read).lines(),
            next_id: 1,
            _dir: dir,
            token,
        };
        let initialized = client
            .request(
                "initialize",
                json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}),
            )
            .await;
        assert!(initialized.get("result").is_some(), "{initialized}");
        client
            .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
        client
    }

    async fn send(&mut self, message: JsonValue) {
        self.writer.write_all(format!("{message}\n").as_bytes()).await.unwrap();
    }

    async fn request(&mut self, method: &str, params: JsonValue) -> JsonValue {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        let line = self.lines.next_line().await.unwrap().expect("a response");
        serde_json::from_str(&line).unwrap()
    }
}

#[tokio::test]
async fn tools_list_is_refused_without_a_token() {
    let mut client = Client::start().await;
    let response = client.request("tools/list", json!({})).await;
    assert!(response.get("error").is_some(), "{response}");
}

#[tokio::test]
async fn tools_list_is_refused_with_a_bad_token() {
    let mut client = Client::start().await;
    let response = client
        .request("tools/list", json!({"_meta": {"authorization": "not-a-token"}}))
        .await;
    assert!(response.get("error").is_some(), "{response}");
}

#[tokio::test]
async fn tools_list_names_every_tool_for_a_valid_token_with_generated_schemas() {
    let mut client = Client::start().await;
    let token = client.token.clone();
    let response = client
        .request("tools/list", json!({"_meta": {"authorization": token}}))
        .await;
    let tools = response["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools.iter().filter_map(|tool| tool["name"].as_str()).collect();
    assert_eq!(names, TOOL_NAMES);
    assert!(
        tools.iter().all(|tool| tool["inputSchema"]["type"] == "object"),
        "{response}"
    );
    assert!(
        tools
            .iter()
            .find(|tool| tool["name"] == "browse_table")
            .unwrap()["inputSchema"]["properties"]["offset"]["oneOf"]
            .is_array(),
        "{response}"
    );
    let hint =
        |name: &str, key: &str| tools.iter().find(|tool| tool["name"] == name).unwrap()["annotations"][key].clone();
    assert_eq!(hint("list_tables", "readOnlyHint"), true, "{response}");
    assert_eq!(hint("execute_write", "readOnlyHint"), false, "{response}");
    assert_eq!(hint("execute_write", "destructiveHint"), true, "{response}");
}

#[tokio::test]
async fn a_tool_call_accepts_the_token_in_the_arguments_like_today() {
    let mut client = Client::start().await;
    let token = client.token.clone();
    let response = client
        .request(
            "tools/call",
            json!({"name": "list_connections", "arguments": {"token": token}}),
        )
        .await;
    assert_eq!(response["result"]["isError"], false, "{response}");
    assert_eq!(response["result"]["content"][0]["text"], "[]");
}

#[tokio::test]
async fn a_tool_call_without_a_token_is_an_error_result_and_runs_nothing() {
    let mut client = Client::start().await;
    let response = client
        .request("tools/call", json!({"name": "list_connections", "arguments": {}}))
        .await;
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert_eq!(response["result"]["structuredContent"]["code"], "request_failed");
    assert_eq!(
        response["result"]["structuredContent"]["operation_outcome"],
        "not_started"
    );
}

#[test]
fn rmcp_transport_serializes_the_same_structured_outcome() {
    let error = crate::errors::McpToolError::from_write_error(&tablepro_core::DriverError::OperationOutcomeUnknown {
        source: Box::new(tablepro_core::DriverError::Disconnected),
    });
    let result = serde_json::to_value(tool_error_result(&error)).unwrap();

    assert_eq!(result["isError"], true);
    assert_eq!(result["structuredContent"], error.structured_content());
}

#[tokio::test]
async fn a_tool_call_with_a_bad_token_is_an_error_result() {
    let mut client = Client::start().await;
    let response = client
        .request(
            "tools/call",
            json!({"name": "list_connections", "arguments": {"token": "nope"}}),
        )
        .await;
    assert_eq!(response["result"]["isError"], true, "{response}");
}

#[tokio::test]
async fn a_top_level_params_token_is_not_seen_by_the_sdk_layer() {
    let mut client = Client::start().await;
    let token = client.token.clone();
    let response = client
        .request(
            "tools/call",
            json!({"name": "list_connections", "arguments": {}, "token": token}),
        )
        .await;
    assert_eq!(
        response["result"]["isError"], true,
        "documents the compatibility gap: params.token needs a transport shim"
    );
}
