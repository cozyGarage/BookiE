#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;

use async_trait::async_trait;
use bookie_core::Connection;
use bookie_mcp::{ConnectionProvider, McpBridge, TokenPermissions, TokenStore, serve_unix_socket_until};
use bookie_policy::Principal;
use bookie_storage::SavedConnection;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct NoProvider;

#[async_trait]
impl ConnectionProvider for NoProvider {
    async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
        Ok(Vec::new())
    }

    async fn connection(&self, _: Uuid, _: Principal) -> Result<Arc<dyn Connection>, String> {
        Err("no connections in this test".into())
    }
}

fn relay_binary() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_bookie-agentd")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_bookie-agentd").into())
        .into()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_shipped_relay_command_carries_an_mcp_session_to_the_socket() {
    let directory = tempfile::tempdir().unwrap();
    let tokens = Arc::new(TokenStore::open(directory.path().join("tokens.json")).unwrap());
    let (_, token) = tokens
        .issue("relay".into(), TokenPermissions::ReadOnly, vec![Uuid::new_v4()], None)
        .unwrap();
    let bridge = Arc::new(McpBridge::new(Arc::new(NoProvider), tokens));
    let path = directory.path().join("run").join("mcp.sock");
    let shutdown = CancellationToken::new();
    let server = tokio::spawn(serve_unix_socket_until(bridge, path.clone(), shutdown.clone()));
    for _ in 0..200 {
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let relayed = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(relay_binary())
            .arg("--connect-socket")
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut ask = |request: Value| -> Value {
            writeln!(stdin, "{request}").unwrap();
            let mut line = String::new();
            stdout.read_line(&mut line).unwrap();
            serde_json::from_str(&line).unwrap_or_else(|_| panic!("not JSON: {line:?}"))
        };
        let initialized = ask(json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}
        }));
        let listed = ask(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {"token": token}}));
        drop(stdin);
        let status = child.wait().unwrap();
        (initialized, listed, status)
    })
    .await
    .unwrap();
    shutdown.cancel();
    server.await.unwrap().unwrap();

    let (initialized, listed, status) = relayed;
    assert!(initialized["result"]["serverInfo"].is_object(), "{initialized}");
    assert!(
        listed["result"]["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty()),
        "{listed}"
    );
    assert!(
        status.success(),
        "the relay exits cleanly when the client closes its input: {status}"
    );
}

#[test]
fn the_relay_says_so_and_fails_when_no_app_is_listening() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(relay_binary())
        .arg("--connect-socket")
        .arg(directory.path().join("missing.sock"))
        .stdin(Stdio::null())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot reach the BookiE MCP socket"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
