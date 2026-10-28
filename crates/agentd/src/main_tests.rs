#[test]
fn only_a_single_y_allows_a_terminal_approval() {
    for allowed in ["y", "Y", "y\n", " y \r\n", "\ty"] {
        assert_eq!(outcome_for_answer(allowed), ApprovalOutcome::AllowOnce, "{allowed:?}");
    }
    for denied in ["", "\n", "n", "yes", "yy", "ny", "y n", "1", "ok", "\u{ff59}"] {
        assert_eq!(outcome_for_answer(denied), ApprovalOutcome::Deny, "{denied:?}");
    }
}

use super::*;
use tablepro_core::{AuthMode, Environment};

#[test]
fn sanitize_for_terminal_strips_ansi_escapes_and_carriage_returns() {
    // The exact shape of the bypass: erase the displayed rule/reason
    // with CR and an ANSI clear-line, then redraw a fake benign prompt.
    let hostile = "\x1b[2K\rApprove? [y/N] y\x1b[0m";
    let sanitized = sanitize_for_terminal(hostile);
    assert!(!sanitized.contains('\x1b'));
    assert!(!sanitized.contains('\r'));
    assert_eq!(sanitized.chars().filter(|c| *c == '\u{FFFD}').count(), 3);
}

#[test]
fn sanitize_for_terminal_leaves_ordinary_sql_untouched() {
    let sql = "SELECT pan FROM cards WHERE id = 1";
    assert_eq!(sanitize_for_terminal(sql), sql);
}

#[tokio::test]
async fn an_unanswered_prompt_is_denied_after_the_timeout_instead_of_hanging_forever() {
    // The blocking closure outlives the timeout but is still bounded:
    // tokio's runtime waits for spawned blocking work to finish on
    // shutdown, so an unbounded sleep here would hang the test process.
    let started = std::time::Instant::now();
    let outcome = approval_with_timeout(std::time::Duration::from_millis(20), || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        ApprovalOutcome::AllowOnce
    })
    .await;
    assert_eq!(outcome, ApprovalOutcome::Deny);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

fn saved_connection(id: Uuid) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id,
        name: "test".into(),
        driver_id: "postgres".into(),
        host: "localhost".into(),
        port: 5432,
        socket_dir: None,
        socket_path: None,
        database: "postgres".into(),
        username: "postgres".into(),
        use_tls: false,
        tls_mode: None,
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: false,
        auth_mode: AuthMode::Password,
        environment: Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
    }
}

#[test]
fn an_issued_token_is_read_only_unless_asked_otherwise() {
    let id = Uuid::new_v4();
    let args = Args::parse_from([
        "tablepro-agentd",
        "--policy",
        "policy.toml",
        "--issue-token",
        "agent",
        "--connection",
        &id.to_string(),
    ]);
    assert!(matches!(args.permissions, TokenScope::ReadOnly));
    assert!(args.expires_days.is_none());
    assert!(args.token_file.is_none());

    let elevated = Args::parse_from([
        "tablepro-agentd",
        "--policy",
        "policy.toml",
        "--issue-token",
        "agent",
        "--connection",
        &id.to_string(),
        "--permissions",
        "read-write",
        "--expires-days",
        "7",
    ]);
    assert!(matches!(elevated.permissions, TokenScope::ReadWrite));
    assert_eq!(elevated.expires_days, Some(7));
}

#[test]
fn a_token_file_is_written_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("token");
    write_token_file(&path, "secret").unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "token file mode was {mode:o}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "secret");
}

#[test]
fn an_existing_loose_token_file_is_made_owner_only_before_the_token_is_written() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("token");
    std::fs::write(&path, "old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    write_token_file(&path, "secret").unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "the token was left readable with mode {mode:o}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "secret");
}

#[test]
fn sanitize_for_terminal_removes_text_direction_overrides_and_invisible_characters() {
    let hostile = "DROP TABLE a\u{202E}; SELECT 1 --\u{2066}x\u{2069}\u{200B}y\u{2028}z\u{FEFF}\u{061C}\u{2060}";
    let sanitized = sanitize_for_terminal(hostile);
    for c in sanitized.chars() {
        assert!(
            !matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200B}'..='\u{200F}' | '\u{2028}' | '\u{2029}' | '\u{FEFF}' | '\u{061C}' | '\u{2060}'),
            "{:?} survived in {sanitized:?}",
            c
        );
    }
    assert!(sanitized.contains("DROP TABLE a") && sanitized.contains("SELECT 1"));
}

#[test]
fn sanitize_for_terminal_keeps_ordinary_unicode_text() {
    for text in ["SELECT 'café', '日本語', '😀' FROM t", "名前 = 'Zoë'"] {
        assert_eq!(sanitize_for_terminal(text), text);
    }
}

#[test]
fn the_daemon_exposes_no_network_bind_option() {
    for flag in ["--bind", "--host", "--port", "--listen", "--http", "--address"] {
        let parsed = Args::try_parse_from(["tablepro-agentd", "--policy", "policy.toml", flag, "0.0.0.0"]);
        assert!(parsed.is_err(), "{flag} must not be accepted");
    }
    let stdio_only = Args::try_parse_from(["tablepro-agentd", "--policy", "policy.toml"]);
    assert!(stdio_only.is_ok(), "the daemon serves stdio with only a policy path");
}

#[test]
fn token_connections_require_at_least_one_saved_connection() {
    let id = Uuid::new_v4();
    let saved = vec![saved_connection(id)];

    assert!(validate_token_connections(&[], &saved).is_err());
    assert!(validate_token_connections(&[Uuid::new_v4()], &saved).is_err());
    assert!(validate_token_connections(&[id], &saved).is_ok());
}
