use std::sync::Arc;

use super::{McpBridge, McpLimits, count_from_value, count_statement, sql_looks_like_write, validated_identifier};
use crate::tokens::TokenStore;
use tablepro_core::Value;

struct NoProvider;

#[async_trait::async_trait]
impl super::ConnectionProvider for NoProvider {
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

#[test]
fn bridge_shutdown_cancels_every_owned_operation_control() {
    let dir = tempfile::tempdir().unwrap();
    let tokens = Arc::new(TokenStore::open(dir.path().join("tokens.json")).unwrap());
    let shutdown = tokio_util::sync::CancellationToken::new();
    let bridge = McpBridge::with_shutdown_token(Arc::new(NoProvider), tokens, shutdown);
    let control = bridge.operation_control();

    bridge.shutdown();

    assert!(control.cancellation_token().is_cancelled());
    assert!(bridge.operation_control().cancellation_token().is_cancelled());
}

/// H-adjacent finding: a failed authenticate never resolves to a token
/// id, so it never reached the per-token rate limiter and could be
/// retried without bound, each attempt taking the token store's
/// cross-process file lock.
#[test]
fn repeated_failed_authentication_is_rate_limited_even_with_no_valid_token() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(TokenStore::open(dir.path().join("tokens.json")).unwrap());
    let bridge = McpBridge::with_limits(
        Arc::new(NoProvider),
        store,
        McpLimits {
            requests_per_minute: 2,
            ..McpLimits::default()
        },
    );

    assert!(bridge.authenticate("guess-1").is_err());
    assert!(bridge.authenticate("guess-2").is_err());
    let third = bridge.authenticate("guess-3").unwrap_err();
    assert!(third.contains("rate limit"), "{third}");
}

#[test]
fn rate_limited_guesses_do_not_wait_for_the_token_store_lock() {
    use std::fs::OpenOptions;
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(TokenStore::open(dir.path().join("tokens.json")).unwrap());
    let bridge = Arc::new(McpBridge::with_limits(
        Arc::new(NoProvider),
        store.clone(),
        McpLimits {
            requests_per_minute: 1,
            ..McpLimits::default()
        },
    ));
    assert!(bridge.authenticate("guess-1").is_err());

    let lock_path = store.path().with_file_name("tokens.json.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .unwrap();
    lock.lock().unwrap();
    let (started, was_started) = mpsc::channel();
    let (completed, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        started.send(()).unwrap();
        completed.send(bridge.authenticate("guess-2")).unwrap();
    });
    was_started.recv().unwrap();
    let outcome = result.recv_timeout(Duration::from_secs(1));
    drop(lock);
    worker.join().unwrap();

    assert!(matches!(outcome, Ok(Err(error)) if error.contains("rate limit")));
}

#[test]
fn valid_tokens_are_not_throttled_by_the_failed_attempt_budget() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(TokenStore::open(dir.path().join("tokens.json")).unwrap());
    let (_meta, plaintext) = store
        .issue("agent".into(), crate::auth::TokenPermissions::ReadOnly, vec![], None)
        .unwrap();
    let bridge = McpBridge::with_limits(
        Arc::new(NoProvider),
        store,
        McpLimits {
            requests_per_minute: 2,
            ..McpLimits::default()
        },
    );

    for _ in 0..5 {
        bridge
            .authenticate(&plaintext)
            .expect("a valid token keeps authenticating");
    }
    assert!(bridge.authenticate("guess-1").is_err());
    assert!(bridge.authenticate("guess-2").is_err());
    let third = bridge.authenticate("guess-3").unwrap_err();
    assert!(third.contains("rate limit"), "{third}");
}

#[test]
fn a_valid_token_still_authenticates_after_the_failed_attempt_budget_is_spent() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(TokenStore::open(dir.path().join("tokens.json")).unwrap());
    let (_meta, plaintext) = store
        .issue("agent".into(), crate::auth::TokenPermissions::ReadOnly, vec![], None)
        .unwrap();
    let bridge = McpBridge::with_limits(
        Arc::new(NoProvider),
        store,
        McpLimits {
            requests_per_minute: 2,
            ..McpLimits::default()
        },
    );

    for guess in ["guess-1", "guess-2", "guess-3", "guess-4"] {
        assert!(bridge.authenticate(guess).is_err());
    }
    bridge
        .authenticate(&plaintext)
        .expect("another caller's failures cannot lock out a valid token");
}

#[test]
fn a_token_issued_by_another_store_survives_a_spent_failed_attempt_budget() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tokens.json");
    let store = Arc::new(TokenStore::open(path.clone()).unwrap());
    let issuer = TokenStore::open(path).unwrap();
    let bridge = McpBridge::with_limits(
        Arc::new(NoProvider),
        store,
        McpLimits {
            requests_per_minute: 2,
            ..McpLimits::default()
        },
    );

    for guess in ["guess-1", "guess-2", "guess-3", "guess-4"] {
        assert!(bridge.authenticate(guess).is_err());
    }
    let (_, plaintext) = issuer
        .issue("agent".into(), crate::auth::TokenPermissions::ReadOnly, vec![], None)
        .unwrap();

    bridge
        .authenticate(&plaintext)
        .expect("a token added by another process is still valid");
}

#[test]
fn a_count_statement_quotes_the_target_for_the_engine() {
    assert_eq!(
        count_statement("postgres", Some("public"), "items").unwrap(),
        "SELECT COUNT(*) AS row_count FROM \"public\".\"items\""
    );
    assert_eq!(
        count_statement("mysql", None, "items").unwrap(),
        "SELECT COUNT(*) AS row_count FROM `items`"
    );
    assert_eq!(
        count_statement("mssql", None, "it]ems").unwrap(),
        "SELECT COUNT(*) AS row_count FROM [it]]ems]"
    );
}

#[test]
fn a_hostile_identifier_cannot_escape_its_quotes() {
    let sql = count_statement("postgres", None, "items\"; TRUNCATE users --").unwrap();
    assert_eq!(
        sql,
        "SELECT COUNT(*) AS row_count FROM \"items\"\"; TRUNCATE users --\""
    );
    assert!(validated_identifier("").is_err());
    assert!(validated_identifier("  ").is_err());
    assert!(validated_identifier("items\nitems").is_err());
    assert!(validated_identifier("items\0").is_err());
    assert!(validated_identifier(&"x".repeat(1024)).is_err());
}

#[test]
fn only_a_non_negative_integer_count_is_accepted() {
    assert_eq!(count_from_value(&Value::Int(7)).unwrap(), 7);
    assert_eq!(count_from_value(&Value::Decimal(12.into())).unwrap(), 12);
    assert!(count_from_value(&Value::Int(-1)).is_err());
    assert!(count_from_value(&Value::Text("7".into())).is_err());
    assert!(count_from_value(&Value::Null).is_err());
}

#[test]
fn the_default_limits_are_the_values_the_bridge_has_always_used() {
    let limits = McpLimits::default();
    assert_eq!(limits.requests_per_minute, 120);
    assert_eq!(limits.max_rows, 500);
    assert_eq!(limits.query_timeout_secs, 30);
}

#[test]
fn only_a_provable_read_skips_the_write_scope_check() {
    for sql in ["SELECT 1", "SELECT id FROM t WHERE id = 1"] {
        assert!(!sql_looks_like_write(sql, "postgres"), "{sql}");
    }
    for sql in [
        "DELETE FROM t WHERE id = 1",
        "CREATE TABLE t (id int)",
        "SELECT pg_read_file('/etc/passwd')",
        "COPY t TO PROGRAM 'sh'",
        "this is not sql at all",
        "",
    ] {
        assert!(sql_looks_like_write(sql, "postgres"), "{sql}");
    }
}
