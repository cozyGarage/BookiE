#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tablepro_core::Connection;
use tablepro_mcp::{ConnectionProvider, McpBridge, McpLimits, TokenPermissions, TokenStore, dispatch};
use tablepro_policy::Principal;
use tablepro_storage::SavedConnection;
use uuid::Uuid;

struct NoConnections;

#[async_trait]
impl ConnectionProvider for NoConnections {
    async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
        Ok(Vec::new())
    }

    async fn connection(&self, _: Uuid, _: Principal) -> Result<Arc<dyn Connection>, String> {
        Err("no connections in this test".into())
    }
}

fn bridge_and_plaintext(limit: u32) -> (McpBridge, String, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let tokens = Arc::new(TokenStore::open(directory.path().join("tokens.json")).unwrap());
    let (_metadata, plaintext) = tokens
        .issue("limited".into(), TokenPermissions::ReadOnly, vec![Uuid::new_v4()], None)
        .unwrap();
    let bridge = McpBridge::with_limits(
        Arc::new(NoConnections),
        tokens,
        McpLimits {
            requests_per_minute: limit,
            ..McpLimits::default()
        },
    );
    (bridge, plaintext, directory)
}

#[tokio::test]
async fn malformed_tool_calls_count_against_the_token_rate_limit() {
    let (bridge, plaintext, _directory) = bridge_and_plaintext(2);
    let token = bridge.authenticate(&plaintext).unwrap();

    let first = dispatch(&bridge, &token, "execute_query", json!({})).await;
    let second = dispatch(&bridge, &token, "execute_query", json!({})).await;
    let third = dispatch(&bridge, &token, "execute_query", json!({})).await;

    assert!(!first.unwrap_err().contains("rate limit"));
    assert!(!second.unwrap_err().contains("rate limit"));
    assert!(third.unwrap_err().contains("rate limit"));
}

#[tokio::test]
async fn an_unknown_tool_name_counts_against_the_token_rate_limit() {
    let (bridge, plaintext, _directory) = bridge_and_plaintext(1);
    let token = bridge.authenticate(&plaintext).unwrap();

    let first = dispatch(&bridge, &token, "no_such_tool", json!({})).await;
    let second = dispatch(&bridge, &token, "no_such_tool", json!({})).await;

    assert!(!first.unwrap_err().contains("rate limit"));
    assert!(second.unwrap_err().contains("rate limit"));
}

#[tokio::test]
async fn a_well_formed_call_is_counted_once() {
    let (bridge, plaintext, _directory) = bridge_and_plaintext(2);
    let token = bridge.authenticate(&plaintext).unwrap();

    let first = dispatch(&bridge, &token, "list_connections", json!({})).await;
    let second = dispatch(&bridge, &token, "list_connections", json!({})).await;
    let third = dispatch(&bridge, &token, "list_connections", json!({})).await;

    assert!(first.is_ok() && second.is_ok(), "{first:?} {second:?}");
    assert!(third.unwrap_err().contains("rate limit"));
}
