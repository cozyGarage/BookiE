//! In-process MCP server for the GTK app. Agents talk to the same
//! policy-gated connections the UI uses.

use std::{
    future::Future,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use bookie_core::Connection;
use bookie_mcp::{ConnectionProvider, McpBridge, McpServerConfig, TokenPermissions, TokenStore};
use bookie_policy::Principal;
use bookie_storage::{SavedConnection, StorageError, load_connections, store_mcp_token};
use chrono::{DateTime, Duration, Utc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::database_service::DatabaseService;

struct AppConnectionProvider {
    database: Arc<DatabaseService>,
}

#[async_trait]
impl ConnectionProvider for AppConnectionProvider {
    async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
        load_connections().await.map_err(|e| e.to_string())
    }

    async fn connection(&self, connection_id: Uuid, principal: Principal) -> Result<Arc<dyn Connection>, String> {
        let svc = &self.database;
        let conn = match &principal {
            Principal::Human { .. } => svc.get(connection_id),
            _ => svc.handle(connection_id, principal),
        };
        conn.ok_or_else(|| format!("connection {connection_id} is not open in the app"))
    }
}

/// Start the loopback MCP HTTP server.
pub struct RunningMcpServer {
    pub bridge: Arc<McpBridge>,
    shutdown: CancellationToken,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl RunningMcpServer {
    pub fn shutdown(mut self) {
        self.shutdown.cancel();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::warn!("MCP server thread panicked during shutdown");
        }
    }
}

pub fn start_background(database: Arc<DatabaseService>) -> Option<RunningMcpServer> {
    if !database.audit_available() {
        tracing::error!("MCP server disabled because the required audit journal is unavailable");
        return None;
    }
    if !database.policy_available() {
        tracing::error!("MCP server disabled because the policy file could not be loaded");
        return None;
    }
    let tokens = match TokenStore::open_default() {
        Ok(t) => Arc::new(t),
        Err(e) => {
            tracing::warn!(error = %e, "MCP token store unavailable");
            return None;
        }
    };
    let shutdown = CancellationToken::new();
    let bridge = Arc::new(McpBridge::with_shutdown_token(
        Arc::new(AppConnectionProvider { database }),
        tokens,
        shutdown.clone(),
    ));
    let bridge_http = bridge.clone();
    let test_port = std::env::var("BOOKIE_TEST_MCP_HTTP_PORT").ok();
    let config = server_config(test_port.as_deref());
    let server_shutdown = shutdown.clone();
    let port = config.bind_port;
    tracing::info!(host = %config.bind_host, port = config.bind_port, "MCP HTTP server starting");
    let thread = match std::thread::Builder::new().name("bookie-mcp".into()).spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::warn!(error = %e, "MCP runtime unavailable; MCP HTTP server not started");
                return;
            }
        };
        rt.block_on(async move {
            if let Err(e) = bookie_mcp::serve_streamable_http_until(bridge_http, config, server_shutdown).await {
                tracing::warn!(error = %e, "MCP HTTP server stopped");
                record_server_failure(&e, port);
            }
        });
    }) {
        Ok(thread) => thread,
        Err(error) => {
            tracing::warn!(%error, "MCP server thread unavailable");
            return None;
        }
    };
    Some(RunningMcpServer {
        bridge,
        shutdown,
        thread: Some(thread),
    })
}

fn server_config(test_port: Option<&str>) -> McpServerConfig {
    let mut config = McpServerConfig::default();
    if let Some(port) = test_port
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port > 0)
    {
        config.bind_port = port;
    }
    config
}

pub async fn issue_token(
    bridge: Option<&Arc<McpBridge>>,
    name: String,
    permissions: TokenPermissions,
    connection_allowlist: Vec<Uuid>,
    tool_allowlist: Option<Vec<String>>,
    expires_at: Option<DateTime<Utc>>,
) -> Result<IssuedToken, String> {
    issue_token_with_store(
        bridge,
        name,
        permissions,
        connection_allowlist,
        tool_allowlist,
        expires_at,
        |id, plaintext, label| async move { store_mcp_token(id, &plaintext, &label).await },
    )
    .await
}

static SERVER_FAILURE: Mutex<Option<String>> = Mutex::new(None);

fn record_server_failure(error: &str, port: u16) {
    let message = if error.contains("in use") {
        format!(
            "Port {port} is already used by another program, so agents cannot reach BookiE. Close that program and restart BookiE."
        )
    } else {
        format!("The MCP server stopped: {error}")
    };
    *SERVER_FAILURE.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
}

pub fn server_failure() -> Option<String> {
    SERVER_FAILURE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub fn expiry_for_choice(choice: u32, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let days = match choice {
        0 => 30,
        2 => 365,
        3 => return None,
        _ => 90,
    };
    Some(now + Duration::days(days))
}

pub struct IssuedToken {
    pub plaintext: String,
    pub keyring_saved: bool,
}

async fn issue_token_with_store<F, Fut>(
    bridge: Option<&Arc<McpBridge>>,
    name: String,
    permissions: TokenPermissions,
    connection_allowlist: Vec<Uuid>,
    tool_allowlist: Option<Vec<String>>,
    expires_at: Option<DateTime<Utc>>,
    store: F,
) -> Result<IssuedToken, String>
where
    F: FnOnce(Uuid, String, String) -> Fut,
    Fut: Future<Output = Result<(), StorageError>>,
{
    let bridge = bridge.ok_or_else(|| "MCP server is not running".to_string())?;
    let (meta, plaintext) = bridge.tokens().issue_with_tools(
        name.clone(),
        permissions,
        connection_allowlist,
        tool_allowlist,
        expires_at,
    )?;
    let keyring_saved = match store(meta.id, plaintext.clone(), format!("BookiE MCP: {name}")).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(error = %error, "failed to store MCP token in libsecret");
            false
        }
    };
    Ok(IssuedToken {
        plaintext,
        keyring_saved,
    })
}

pub fn revoke_token(bridge: Option<&Arc<McpBridge>>, id: Uuid) -> Result<(), String> {
    let bridge = bridge.ok_or_else(|| "MCP server is not running".to_string())?;
    bridge.tokens().revoke(id)?;
    relm4::spawn(async move {
        if let Err(e) = bookie_storage::delete_mcp_token(id).await {
            tracing::warn!(error = %e, "failed to delete MCP token from libsecret");
        }
    });
    Ok(())
}

pub fn list_tokens(bridge: Option<&Arc<McpBridge>>) -> Vec<bookie_mcp::McpToken> {
    bridge.map(|b| b.tokens().list()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        Duration, Utc, expiry_for_choice, issue_token_with_store, record_server_failure, server_config, server_failure,
    };
    use std::sync::Arc;

    use bookie_core::Connection;
    use bookie_mcp::{ConnectionProvider, McpBridge, TokenPermissions, TokenStore};
    use bookie_storage::{KeyringFailure, SavedConnection, StorageError};
    use uuid::Uuid;

    struct NoConnections;

    #[async_trait::async_trait]
    impl ConnectionProvider for NoConnections {
        async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
            Ok(Vec::new())
        }

        async fn connection(
            &self,
            _connection_id: Uuid,
            _principal: bookie_policy::Principal,
        ) -> Result<Arc<dyn Connection>, String> {
            Err("unused test connection".into())
        }
    }

    fn bridge() -> (tempfile::TempDir, Arc<McpBridge>) {
        let directory = tempfile::tempdir().unwrap();
        let tokens = Arc::new(TokenStore::open(directory.path().join("tokens.json")).unwrap());
        let bridge = Arc::new(McpBridge::new(Arc::new(NoConnections), tokens));
        (directory, bridge)
    }

    #[tokio::test]
    async fn issue_token_reports_successful_keyring_storage() {
        let (_directory, bridge) = bridge();
        let issued = issue_token_with_store(
            Some(&bridge),
            "test client".into(),
            TokenPermissions::ReadOnly,
            vec![Uuid::new_v4()],
            None,
            None,
            |_, _, _| async { Ok::<(), StorageError>(()) },
        )
        .await
        .unwrap();

        assert!(issued.keyring_saved);
        let tokens = bridge.tokens().list();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].tool_allowlist, None);
    }

    #[tokio::test]
    async fn issue_token_remains_usable_when_keyring_storage_fails() {
        let (_directory, bridge) = bridge();
        let issued = issue_token_with_store(
            Some(&bridge),
            "test client".into(),
            TokenPermissions::ReadOnly,
            vec![Uuid::new_v4()],
            None,
            None,
            |_, _, _| async { Err(StorageError::Keyring(KeyringFailure::Unavailable)) },
        )
        .await
        .unwrap();

        assert!(!issued.keyring_saved);
        assert!(bridge.tokens().authenticate(&issued.plaintext).is_ok());
        assert_eq!(bridge.tokens().list()[0].tool_allowlist, None);
    }

    #[tokio::test]
    async fn issue_token_applies_selected_tool_allowlist() {
        let (_directory, bridge) = bridge();
        let issued = issue_token_with_store(
            Some(&bridge),
            "test client".into(),
            TokenPermissions::ReadOnly,
            vec![Uuid::new_v4()],
            Some(vec!["list_tables".into()]),
            None,
            |_, _, _| async { Ok::<(), StorageError>(()) },
        )
        .await
        .unwrap();

        assert!(bridge.tokens().authenticate(&issued.plaintext).is_ok());
        let token = bridge.tokens().list().remove(0);
        assert_eq!(
            token.tool_allowlist.as_deref(),
            Some(["list_tables".to_string()].as_slice())
        );
        assert!(token.allows_tool("list_tables"));
        assert!(!token.allows_tool("execute_query"));
    }

    #[tokio::test]
    async fn issue_token_applies_the_chosen_expiry() {
        let (_directory, bridge) = bridge();
        let expires = Utc::now() + Duration::days(90);
        issue_token_with_store(
            Some(&bridge),
            "test client".into(),
            TokenPermissions::ReadOnly,
            vec![Uuid::new_v4()],
            None,
            Some(expires),
            |_, _, _| async { Ok::<(), StorageError>(()) },
        )
        .await
        .unwrap();

        assert_eq!(bridge.tokens().list()[0].expires_at, Some(expires));
    }

    #[test]
    fn the_expiry_choices_default_to_ninety_days_and_never_is_explicit() {
        let now = Utc::now();
        assert_eq!(expiry_for_choice(0, now), Some(now + Duration::days(30)));
        assert_eq!(expiry_for_choice(1, now), Some(now + Duration::days(90)));
        assert_eq!(expiry_for_choice(2, now), Some(now + Duration::days(365)));
        assert_eq!(expiry_for_choice(3, now), None);
        assert_eq!(expiry_for_choice(99, now), Some(now + Duration::days(90)));
    }

    #[test]
    fn a_taken_port_is_reported_in_plain_words() {
        record_server_failure("Address already in use (os error 98)", 17432);
        let message = server_failure().unwrap();
        assert!(
            message.contains("17432") && message.contains("already used"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn an_mcp_request_never_opens_a_connection_the_window_does_not_hold() {
        use super::AppConnectionProvider;
        let database = Arc::new(crate::services::database_service::DatabaseService::new_isolated());
        let provider = AppConnectionProvider {
            database: database.clone(),
        };
        let unopened = Uuid::new_v4();
        let human = provider
            .connection(unopened, bookie_policy::Principal::human_gui())
            .await;
        let agent = provider
            .connection(
                unopened,
                bookie_policy::Principal::Agent {
                    token: Uuid::new_v4().to_string(),
                    client: Some("test".into()),
                    model: None,
                },
            )
            .await;
        assert!(human.is_err());
        assert!(agent.is_err());
        assert!(!database.is_active(unopened));
    }

    #[test]
    fn test_mcp_port_override_accepts_only_nonzero_u16_values() {
        assert_eq!(server_config(Some("23456")).bind_port, 23456);
        assert_eq!(server_config(Some("0")).bind_port, 17432);
        assert_eq!(server_config(Some("65536")).bind_port, 17432);
        assert_eq!(server_config(None).bind_port, 17432);
    }
}
