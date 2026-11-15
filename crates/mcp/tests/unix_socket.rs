#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bookie_core::Connection;
use bookie_mcp::{
    ConnectionProvider, McpBridge, TokenPermissions, TokenStore, relay_to_socket, serve_unix_socket_until,
};
use bookie_policy::Principal;
use bookie_storage::SavedConnection;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
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

struct Server {
    path: PathBuf,
    token: String,
    shutdown: CancellationToken,
    task: tokio::task::JoinHandle<Result<(), String>>,
    _directory: tempfile::TempDir,
}

async fn start() -> Server {
    let directory = tempfile::tempdir().unwrap();
    let tokens = Arc::new(TokenStore::open(directory.path().join("tokens.json")).unwrap());
    let (_, token) = tokens
        .issue("socket".into(), TokenPermissions::ReadOnly, vec![Uuid::new_v4()], None)
        .unwrap();
    let bridge = Arc::new(McpBridge::new(Arc::new(NoProvider), tokens));
    let path = directory.path().join("run").join("mcp.sock");
    let shutdown = CancellationToken::new();
    let task = tokio::spawn(serve_unix_socket_until(bridge, path.clone(), shutdown.clone()));
    for _ in 0..200 {
        if UnixStream::connect(&path).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Server {
        path,
        token,
        shutdown,
        task,
        _directory: directory,
    }
}

async fn call(path: &std::path::Path, request: Value) -> Value {
    let stream = UnixStream::connect(path).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    writer.write_all(format!("{request}\n").as_bytes()).await.unwrap();
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(5), BufReader::new(reader).read_line(&mut line))
        .await
        .expect("the server answers")
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn the_socket_and_its_directory_are_private_and_the_protocol_works_over_it() {
    let server = start().await;
    let directory_mode = std::fs::metadata(server.path.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    let socket_mode = std::fs::metadata(&server.path).unwrap().permissions().mode();
    assert_eq!(directory_mode & 0o777, 0o700);
    assert_eq!(socket_mode & 0o777, 0o600);

    let refused = call(
        &server.path,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"token":"nope"}}),
    )
    .await;
    assert_eq!(refused["error"]["message"], "authentication failed", "{refused}");

    let listed = call(
        &server.path,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"token":server.token}}),
    )
    .await;
    assert!(
        listed["result"]["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty()),
        "{listed}"
    );

    server.shutdown.cancel();
    server.task.await.unwrap().unwrap();
    assert!(!server.path.exists(), "the socket file is removed on shutdown");
}

#[tokio::test]
async fn a_second_server_on_a_live_socket_is_refused_and_a_stale_socket_is_replaced() {
    let server = start().await;
    let tokens = Arc::new(TokenStore::open(server._directory.path().join("other.json")).unwrap());
    let bridge = Arc::new(McpBridge::new(Arc::new(NoProvider), tokens.clone()));
    let refused = serve_unix_socket_until(bridge.clone(), server.path.clone(), CancellationToken::new()).await;
    assert!(refused.unwrap_err().contains("already serving"));

    server.task.abort();
    let _ = server.task.await;
    assert!(server.path.exists(), "an aborted server leaves a stale socket behind");
    let shutdown = CancellationToken::new();
    let replacement = tokio::spawn(serve_unix_socket_until(bridge, server.path.clone(), shutdown.clone()));
    for _ in 0..200 {
        if UnixStream::connect(&server.path).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        UnixStream::connect(&server.path).await.is_ok(),
        "the stale socket was replaced"
    );
    shutdown.cancel();
    replacement.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_directory_other_users_can_open_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let open = directory.path().join("open");
    std::fs::create_dir(&open).unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
    let tokens = Arc::new(TokenStore::open(directory.path().join("tokens.json")).unwrap());
    let bridge = Arc::new(McpBridge::new(Arc::new(NoProvider), tokens));

    let error = serve_unix_socket_until(bridge, open.join("mcp.sock"), CancellationToken::new())
        .await
        .unwrap_err();

    assert!(error.contains("only you own"), "{error}");
}

#[tokio::test]
async fn the_relay_carries_a_request_from_stdin_to_the_socket_and_the_answer_back() {
    let server = start().await;
    let request = format!(
        "{}\n",
        json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{"token":server.token}})
    );
    let (mut client_in, relay_in) = tokio::io::duplex(1 << 16);
    let (relay_out, mut client_out) = tokio::io::duplex(1 << 16);
    let path = server.path.clone();
    let relay = tokio::spawn(async move { relay_to_socket(&path, relay_in, relay_out).await });

    client_in.write_all(request.as_bytes()).await.unwrap();
    let mut answer = String::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        BufReader::new(&mut client_out).read_line(&mut answer),
    )
    .await
    .expect("the relay returns the answer")
    .unwrap();
    assert!(
        serde_json::from_str::<Value>(&answer).unwrap()["result"]["tools"].is_array(),
        "{answer}"
    );

    drop(client_in);
    let _ = tokio::time::timeout(Duration::from_secs(5), relay).await;
    server.shutdown.cancel();
    server.task.await.unwrap().unwrap();
}
