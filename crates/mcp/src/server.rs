use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::http::{HeaderMap, Uri, header::ORIGIN};
use serde_json::{Value as JsonValue, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::bridge::McpBridge;
use crate::tools::{self, TOOL_NAMES};

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub bind_host: String,
    pub bind_port: u16,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            bind_host: "127.0.0.1".into(),
            bind_port: 17432,
        }
    }
}

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const DISCARD_CHUNK_BYTES: u64 = 8 * 1024;

/// Revision advertised to a client that does not ask for one, and the
/// older revisions still accepted from a client that does.
const PROTOCOL_VERSION: &str = "2026-07-28";
const ACCEPTED_PROTOCOL_VERSIONS: &[&str] = &[PROTOCOL_VERSION, "2025-11-25", "2025-06-18"];

fn negotiate_protocol_version(params: &JsonValue) -> Result<&'static str, String> {
    let Some(requested) = params.get("protocolVersion") else {
        return Ok(PROTOCOL_VERSION);
    };
    let requested = requested
        .as_str()
        .ok_or_else(|| "protocolVersion must be a string".to_string())?;
    ACCEPTED_PROTOCOL_VERSIONS
        .iter()
        .find(|accepted| **accepted == requested)
        .copied()
        .ok_or_else(|| format!("unsupported protocolVersion; this server accepts {ACCEPTED_PROTOCOL_VERSIONS:?}"))
}

fn initialize_result(params: &JsonValue) -> Result<JsonValue, String> {
    let version = negotiate_protocol_version(params)?;
    Ok(json!({
        "protocolVersion": version,
        "capabilities": {"tools": {}},
        "serverInfo": {"name": "tablepro", "version": env!("CARGO_PKG_VERSION")}
    }))
}

fn json_rpc_error(id: &JsonValue, code: i64, message: String) -> JsonValue {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Loopback-only bind resolution. A host that is not a loopback literal
/// is refused before any socket is opened; there is no configuration
/// that reaches a routable interface.
fn loopback_bind_addr(host: &str, port: u16) -> Result<SocketAddr, String> {
    let host = host.trim();
    let literal = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);
    let ip: IpAddr = match literal {
        "localhost" => IpAddr::from([127, 0, 0, 1]),
        other => other
            .parse()
            .map_err(|_| format!("refusing to bind MCP HTTP to a non-loopback host: {other}"))?,
    };
    if !ip.is_loopback() {
        return Err(format!("refusing to bind MCP HTTP outside loopback: {ip}"));
    }
    Ok(SocketAddr::new(ip, port))
}

async fn discard_rest_of_line<R>(reader: &mut R) -> Result<(), String>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut chunk = Vec::new();
    loop {
        chunk.clear();
        let read = reader
            .take(DISCARD_CHUNK_BYTES)
            .read_until(b'\n', &mut chunk)
            .await
            .map_err(|e| e.to_string())?;
        if read == 0 || chunk.last() == Some(&b'\n') {
            return Ok(());
        }
    }
}

/// Minimal MCP-compatible JSON-RPC loop over stdio. Speaks enough of the
/// protocol for Cursor / Claude Code: `initialize`, `tools/list`,
/// `tools/call`. Auth via `Authorization` in `_meta` or `params.token`.
pub async fn serve_stdio(bridge: Arc<McpBridge>) -> Result<(), String> {
    let reader = BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    serve_json_rpc_lines(&bridge, reader, stdout).await
}

async fn serve_json_rpc_lines<R, W>(bridge: &McpBridge, mut reader: R, mut stdout: W) -> Result<(), String>
where
    R: tokio::io::AsyncBufRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut line = String::new();

    loop {
        line.clear();
        let read = (&mut reader)
            .take(MAX_REQUEST_BYTES as u64 + 1)
            .read_line(&mut line)
            .await;
        let n = match read {
            Ok(n) => n,
            // read_line still consumes the malformed bytes before failing
            // the UTF-8 conversion, so the stream is resynced at the next
            // newline -- this can recover the same way an invalid JSON
            // line does, instead of ending the whole session.
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                write_response(
                    &mut stdout,
                    json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {"code": -32700, "message": format!("parse error: {error}")}
                    }),
                )
                .await?;
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        if n == 0 {
            break;
        }
        if n > MAX_REQUEST_BYTES && !line.ends_with('\n') {
            discard_rest_of_line(&mut reader).await?;
            write_response(
                &mut stdout,
                json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32600, "message": format!("request exceeds {MAX_REQUEST_BYTES} bytes")}
                }),
            )
            .await?;
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let request: JsonValue = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                write_response(
                    &mut stdout,
                    json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {"code": -32700, "message": format!("parse error: {e}")}
                    }),
                )
                .await?;
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(JsonValue::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(json!({}));

        let response = match method {
            "initialize" => match initialize_result(&params) {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err(message) => json_rpc_error(&id, -32602, message),
            },
            "notifications/initialized" | "initialized" => continue,
            "tools/list" => match handle_tools_list(bridge, &params) {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err(message) => json_rpc_error(&id, -32000, message),
            },
            "tools/call" => match handle_tool_call(bridge, params).await {
                Ok(content) => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{"type": "text", "text": content.to_string()}],
                        "isError": false
                    }
                }),
                Err(error) => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": tool_error_result(&error)
                }),
            },
            "ping" => json!({"jsonrpc": "2.0", "id": id, "result": {}}),
            other => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("method not found: {other}")}
            }),
        };
        write_response(&mut stdout, response).await?;
    }
    Ok(())
}

/// Shared lookup for every method that requires a token: `params.token`,
/// `params._meta.authorization`, then (for `tools/call`) `arguments.token`.
fn extract_token_str(params: &JsonValue) -> Option<&str> {
    params
        .get("token")
        .and_then(|v| v.as_str())
        .or_else(|| params.pointer("/_meta/authorization").and_then(|v| v.as_str()))
        .or_else(|| params.pointer("/arguments/token").and_then(|v| v.as_str()))
}

async fn handle_tool_call(bridge: &McpBridge, params: JsonValue) -> Result<JsonValue, crate::errors::McpToolError> {
    let name = params.get("name").and_then(|v| v.as_str()).ok_or("missing tool name")?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let token_str = extract_token_str(&params).ok_or("missing token")?;
    let token = bridge
        .authenticate(token_str)
        .map_err(crate::errors::McpToolError::from)?;
    tools::dispatch_structured(bridge, &token, name, args).await
}

fn tool_error_result(error: &crate::errors::McpToolError) -> JsonValue {
    json!({
        "content": [{"type": "text", "text": error.message}],
        "isError": true,
        "structuredContent": error.structured_content(),
    })
}

#[cfg(test)]
mod structured_error_tests {
    use serde_json::json;

    use super::tool_error_result;
    use crate::errors::McpToolError;

    #[test]
    fn legacy_transport_returns_the_shared_structured_error_shape() {
        let error = McpToolError::from_write_error(&tablepro_core::DriverError::OperationOutcomeUnknown {
            source: Box::new(tablepro_core::DriverError::Disconnected),
        });

        assert_eq!(
            tool_error_result(&error),
            json!({
                "content": [{"type": "text", "text": error.message}],
                "isError": true,
                "structuredContent": {
                    "code": "operation_outcome_unknown",
                    "operation_outcome": "unknown",
                    "transaction_outcome": "not_applicable",
                    "message": error.message
                }
            })
        );
    }
}

/// `tools/list` names every tool this server exposes, including ones a
/// least-privilege token could never call. Require the same token
/// `tools/call` does rather than disclosing the catalogue to anyone who can
/// reach the loopback listener.
fn handle_tools_list(bridge: &McpBridge, params: &JsonValue) -> Result<JsonValue, String> {
    let token_str = extract_token_str(params).ok_or("missing token (pass params.token)")?;
    bridge.authenticate(token_str)?;
    Ok(json!({
        "tools": TOOL_NAMES.iter().map(|name| json!({
            "name": name,
            "description": tool_description(name),
            "inputSchema": tool_schema(name),
            "annotations": tool_annotations(name),
        })).collect::<Vec<_>>()
    }))
}

async fn write_response<W>(stdout: &mut W, value: JsonValue) -> Result<(), String>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut line = serde_json::to_string(&value).map_err(|e| e.to_string())?;
    line.push('\n');
    stdout.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
    stdout.flush().await.map_err(|e| e.to_string())
}

pub(crate) fn tool_description(name: &str) -> &'static str {
    match name {
        "list_connections" => "List saved database connections visible to this token",
        "list_tables" => "List tables on a connection",
        "list_objects" => {
            "List routines, triggers, sequences, extensions, roles, types or table privileges on a connection, optionally in one schema"
        }
        "describe_table" => "Describe columns of a table",
        "execute_query" => {
            "Run a read SQL query (writes require tools:write scope and policy approval). Results use value_format=bookie-tagged-v1; each cell is tagged, integers and decimals use exact strings, parsed JSON is recursively tagged, and raw JSON text remains exact under json_text."
        }
        "execute_write" => {
            "Run a write with optional transaction preview (preview=true by default). Results include value_format=bookie-tagged-v1; numeric counts are exact tagged strings."
        }
        "explain_query" => {
            "Run EXPLAIN on a SQL statement; plan rows are withheld from read-scoped agents. Results include value_format=bookie-tagged-v1 and tagged cells."
        }
        "export_data" => {
            "Run a query and return CSV or JSON; CSV responses include the null_marker used for SQL NULL cells. JSON results use value_format=bookie-tagged-v1; tagged cells preserve value kinds and exact numeric digits, parsed JSON is recursively tagged, and raw JSON text remains exact under json_text"
        }
        "table_schema" => "Read the columns, primary key, indexes and foreign keys of a table",
        "count_rows" => {
            "Count the rows of a table exactly; the result includes value_format=bookie-tagged-v1 and an exact tagged count string"
        }
        "browse_table" => {
            "Read a page of table rows by offset and limit. Pass large offset or limit values as decimal strings. Results include value_format=bookie-tagged-v1; each cell is tagged and numeric digits remain exact strings"
        }
        _ => "",
    }
}

const READ_ONLY_TOOLS: &[&str] = &[
    "list_connections",
    "list_tables",
    "list_objects",
    "describe_table",
    "table_schema",
    "count_rows",
    "browse_table",
];

pub(crate) fn tool_annotations(name: &str) -> JsonValue {
    if READ_ONLY_TOOLS.contains(&name) {
        json!({"readOnlyHint": true, "openWorldHint": false})
    } else {
        json!({"readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false})
    }
}

pub(crate) fn tool_schema(name: &str) -> JsonValue {
    match name {
        "list_connections" => json!({"type": "object", "properties": {}}),
        "list_tables" | "execute_query" | "execute_write" | "explain_query" | "export_data" => json!({
            "type": "object",
            "properties": {
                "connection_id": {"type": "string"},
                "sql": {"type": "string"},
                "preview": {"type": "boolean"},
                "format": {"type": "string"},
                "token": {"type": "string"}
            },
            "required": ["connection_id"]
        }),
        "list_objects" => json!({
            "type": "object",
            "properties": {
                "connection_id": {"type": "string"},
                "kind": {
                    "type": "string",
                    "enum": tablepro_core::CatalogObjectKind::ALL.iter().map(|kind| kind.as_str()).collect::<Vec<_>>()
                },
                "schema": {"type": "string"},
                "token": {"type": "string"}
            },
            "required": ["connection_id", "kind"]
        }),
        "describe_table" | "table_schema" | "count_rows" => json!({
            "type": "object",
            "properties": {
                "connection_id": {"type": "string"},
                "schema": {"type": "string"},
                "table": {"type": "string"},
                "token": {"type": "string"}
            },
            "required": ["connection_id", "table"]
        }),
        "browse_table" => json!({
            "type": "object",
            "properties": {
                "connection_id": {"type": "string"},
                "schema": {"type": "string"},
                "table": {"type": "string"},
                "offset": {"oneOf": [{"type": "integer", "minimum": 0}, {"type": "string", "pattern": "^[0-9]+$"}]},
                "limit": {"oneOf": [{"type": "integer", "minimum": 1}, {"type": "string", "pattern": "^[0-9]+$"}]},
                "token": {"type": "string"}
            },
            "required": ["connection_id", "table"]
        }),
        _ => json!({"type": "object"}),
    }
}

fn origin_is_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    if origin.is_empty() {
        return true;
    }
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return false;
    }
    matches!(
        uri.host().map(str::to_ascii_lowercase).as_deref(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "claude.ai" | "app.cursor.com")
    )
}

/// Bind a loopback streamable-HTTP endpoint that forwards tool calls to
/// the same bridge. Loopback only by default.
pub async fn serve_streamable_http(bridge: Arc<McpBridge>, config: McpServerConfig) -> Result<(), String> {
    serve_streamable_http_until(bridge, config, tokio_util::sync::CancellationToken::new()).await
}

/// Extra time given to the graceful-shutdown race beyond the longest write
/// this bridge admits, so a write already in flight when shutdown begins
/// gets to finish its own bounded cancellation and reach a terminal audit
/// outcome before the runtime hosting it is torn down. A caller that drops
/// this future's loser branch does not stop an already-spawned request task,
/// but a caller that also drops the runtime (`tablepro-app`'s dedicated MCP
/// thread) does, so this deadline must never be shorter than the operation
/// bound `McpBridge` itself already enforces.
const SHUTDOWN_TEARDOWN_MARGIN: std::time::Duration = std::time::Duration::from_secs(5);

pub async fn serve_streamable_http_until(
    bridge: Arc<McpBridge>,
    config: McpServerConfig,
    shutdown: tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    let addr = loopback_bind_addr(&config.bind_host, config.bind_port)?;
    let force_deadline_duration = std::time::Duration::from_secs(bridge.query_timeout_secs) + SHUTDOWN_TEARDOWN_MARGIN;
    let app = http_router(bridge);
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| e.to_string())?;
    tracing::info!(%addr, "MCP HTTP listening (loopback)");
    let server = axum::serve(listener, app).with_graceful_shutdown(shutdown.clone().cancelled_owned());
    let force_deadline = async move {
        shutdown.cancelled().await;
        tokio::time::sleep(force_deadline_duration).await;
    };
    tokio::select! {
        result = server => result.map_err(|error| error.to_string()),
        _ = force_deadline => Err("MCP server shutdown deadline exceeded".into()),
    }
}

fn http_router(bridge: Arc<McpBridge>) -> axum::Router {
    use axum::extract::DefaultBodyLimit;
    use axum::response::IntoResponse;
    use axum::{Json, Router, http::StatusCode, routing::post};

    Router::new()
        .route(
            "/mcp",
            post(move |headers: HeaderMap, Json(body): Json<JsonValue>| {
                let bridge = bridge.clone();
                async move {
                    let cancellation = bridge.request_cancellation();
                    let request_cancellation = cancellation.clone();
                    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                    let operation = crate::bridge::scope_request_cancellation(
                        request_cancellation,
                        process_http_request(bridge, headers, body),
                    );
                    tokio::spawn(supervise_disconnect_aware(response_tx, cancellation, operation));
                    response_rx.await.unwrap_or_else(|_| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({"error": "MCP request task stopped before producing a response"})),
                        )
                            .into_response()
                    })
                }
            }),
        )
        // Matches stdio's own MAX_REQUEST_BYTES instead of axum's larger
        // default, so the sql argument (and everything else in the body)
        // is bounded the same way regardless of which transport carried it.
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
}

async fn process_http_request(bridge: Arc<McpBridge>, headers: HeaderMap, body: JsonValue) -> axum::response::Response {
    use axum::response::IntoResponse;
    use axum::{Json, http::StatusCode};

    if !origin_is_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {"code": -32000, "message": "forbidden origin"}
            })),
        )
            .into_response();
    }
    let id = body.get("id").cloned().unwrap_or(JsonValue::Null);
    let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = body.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        "tools/call" => match handle_tool_call(&bridge, params).await {
            Ok(content) => Ok(json!({
                "content": [{"type": "text", "text": content.to_string()}],
                "isError": false
            })),
            Err(error) => Ok(tool_error_result(&error)),
        },
        "tools/list" => handle_tools_list(&bridge, &params),
        "initialize" => match initialize_result(&params) {
            Ok(result) => Ok(result),
            Err(message) => return Json(json_rpc_error(&id, -32602, message)).into_response(),
        },
        other => Err(format!("unsupported method: {other}")),
    };
    let response = match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err(error) => json_rpc_error(&id, -32000, error),
    };
    Json(response).into_response()
}

async fn supervise_disconnect_aware<T, F>(
    mut response: tokio::sync::oneshot::Sender<T>,
    cancellation: tokio_util::sync::CancellationToken,
    operation: F,
) where
    F: std::future::Future<Output = T>,
{
    tokio::pin!(operation);
    tokio::select! {
        biased;
        _ = response.closed() => {
            cancellation.cancel();
            let _ = operation.await;
        }
        result = &mut operation => {
            let _ = response.send(result);
        }
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
