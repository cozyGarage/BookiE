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
use bookie_core::{AuthMode, Environment};

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
        attached_databases: Vec::new(),
        tls_strict_encryption: false,
    }
}

#[test]
fn an_issued_token_is_read_only_unless_asked_otherwise() {
    let id = Uuid::new_v4();
    let args = Args::parse_from([
        "bookie-agentd",
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
        "bookie-agentd",
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
fn a_token_file_path_that_is_a_symlink_is_refused_and_its_target_is_untouched() {
    let dir = tempfile::TempDir::new().unwrap();
    let target = dir.path().join("precious");
    std::fs::write(&target, "keep me").unwrap();
    let link = dir.path().join("token");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    assert!(write_token_file(&link, "secret").is_err());

    assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep me");
}

#[test]
fn a_token_file_path_that_is_not_a_regular_file_is_refused() {
    let dir = tempfile::TempDir::new().unwrap();
    assert!(write_token_file(dir.path(), "secret").is_err());
}

#[test]
fn rewriting_a_longer_token_file_leaves_no_old_bytes_behind() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("token");
    std::fs::write(&path, "a much longer previous token value").unwrap();
    write_token_file(&path, "new").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
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
        let parsed = Args::try_parse_from(["bookie-agentd", "--policy", "policy.toml", flag, "0.0.0.0"]);
        assert!(parsed.is_err(), "{flag} must not be accepted");
    }
    let stdio_only = Args::try_parse_from(["bookie-agentd", "--policy", "policy.toml"]);
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

#[test]
fn every_registered_driver_is_listed_in_the_feature_table_with_its_declared_maturity() {
    let table = include_str!("../../../docs/features.md");
    let rows: Vec<Vec<&str>> = table
        .lines()
        .filter(|line| line.starts_with("| "))
        .map(|line| line.split('|').map(str::trim).skip(1).collect())
        .collect();
    let registry = build_registry();
    assert!(registry.len() >= 9, "the registry lost a driver");
    for driver in registry.iter() {
        let row = rows
            .iter()
            .find(|cells| {
                cells
                    .first()
                    .is_some_and(|name| name.starts_with(driver.display_name()))
            })
            .unwrap_or_else(|| {
                panic!(
                    "{} is registered but missing from docs/features.md",
                    driver.display_name()
                )
            });
        assert_eq!(
            row.get(1).copied(),
            Some(driver.capabilities().maturity.label()),
            "{} maturity differs between the driver and docs/features.md",
            driver.display_name()
        );
    }
}
