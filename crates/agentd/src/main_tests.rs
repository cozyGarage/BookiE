#[test]
fn only_the_current_prompt_answer_allows_a_terminal_approval() {
    for allowed in ["approve 8b7d", " approve 8b7d \r\n"] {
        assert_eq!(
            outcome_for_answer(allowed, "8b7d"),
            ApprovalOutcome::AllowOnce,
            "{allowed:?}"
        );
    }
    for denied in ["", "\n", "n", "yes", "yy", "ny", "y n", "1", "ok", "\u{ff59}"] {
        assert_eq!(outcome_for_answer(denied, "8b7d"), ApprovalOutcome::Deny, "{denied:?}");
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
#[test]
fn an_approval_answer_must_match_the_current_prompt() {
    assert_eq!(outcome_for_answer("approve 8b7d", "8b7d"), ApprovalOutcome::AllowOnce);
    assert_eq!(outcome_for_answer("approve 8b7d", "other"), ApprovalOutcome::Deny);
}

#[tokio::test]
async fn cancelling_a_prompt_keeps_the_terminal_locked_until_the_reader_exits() {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let (started, wait_started) = tokio::sync::oneshot::channel();
    let (stopping, wait_stopping) = tokio::sync::oneshot::channel();
    let (release, wait_release) = std::sync::mpsc::channel();
    let first = tokio::spawn(terminal_approval(move |cancelled| {
        let _ = started.send(());
        while !cancelled.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = stopping.send(());
        let _ = wait_release.recv_timeout(Duration::from_secs(2));
        ApprovalOutcome::Deny
    }));
    tokio::time::timeout(Duration::from_secs(2), wait_started)
        .await
        .unwrap()
        .unwrap();
    first.abort();
    let _ = first.await;
    tokio::time::timeout(Duration::from_secs(2), wait_stopping)
        .await
        .unwrap()
        .unwrap();
    let (entered, mut wait_entered) = tokio::sync::oneshot::channel();
    let second = tokio::spawn(terminal_approval(move |_| {
        let _ = entered.send(());
        ApprovalOutcome::Deny
    }));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut wait_entered)
            .await
            .is_err()
    );
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), wait_entered)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.await.unwrap(), ApprovalOutcome::Deny);
}

#[tokio::test]
async fn a_late_terminal_answer_cannot_approve_the_next_request() {
    use std::io::Write;
    use std::os::fd::FromRawFd;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    let (mut master_fd, mut slave_fd) = (-1, -1);
    let result = unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(result, 0);
    let mut master = unsafe { std::fs::File::from_raw_fd(master_fd) };
    let mut first_terminal = unsafe { std::fs::File::from_raw_fd(slave_fd) };
    let mut second_terminal = first_terminal.try_clone().unwrap();
    let (first_ready, wait_first) = tokio::sync::oneshot::channel();
    let first = tokio::spawn(terminal_approval(move |cancelled| {
        tty_answer::discard_pending_input(&first_terminal);
        let _ = first_ready.send(());
        tty_answer::read_answer(
            &mut first_terminal,
            Instant::now() + Duration::from_secs(5),
            &|| cancelled.load(Ordering::Acquire),
            "old",
        )
        .unwrap_or(ApprovalOutcome::Deny)
    }));
    tokio::time::timeout(Duration::from_secs(2), wait_first)
        .await
        .unwrap()
        .unwrap();
    first.abort();
    let _ = first.await;
    let (second_ready, wait_second) = tokio::sync::oneshot::channel();
    let second = tokio::spawn(terminal_approval(move |cancelled| {
        tty_answer::discard_pending_input(&second_terminal);
        let _ = second_ready.send(());
        tty_answer::read_answer(
            &mut second_terminal,
            Instant::now() + Duration::from_secs(5),
            &|| cancelled.load(Ordering::Acquire),
            "new",
        )
        .unwrap_or(ApprovalOutcome::Deny)
    }));
    tokio::time::timeout(Duration::from_secs(2), wait_second)
        .await
        .unwrap()
        .unwrap();
    master.write_all(b"approve old\n").unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), second)
            .await
            .unwrap()
            .unwrap(),
        ApprovalOutcome::Deny
    );
}

async fn hold_the_terminal(
    for_millis: u64,
) -> (
    tokio::task::JoinHandle<ApprovalOutcome>,
    tokio::sync::oneshot::Receiver<()>,
) {
    let (held, wait_held) = tokio::sync::oneshot::channel();
    let holder = tokio::spawn(terminal_approval(move |_| {
        let _ = held.send(());
        std::thread::sleep(std::time::Duration::from_millis(for_millis));
        ApprovalOutcome::Deny
    }));
    (holder, wait_held)
}

#[tokio::test]
async fn a_request_that_cannot_get_the_terminal_in_time_is_denied_without_asking() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    let (holder, wait_held) = hold_the_terminal(600).await;
    tokio::time::timeout(Duration::from_secs(10), wait_held)
        .await
        .unwrap()
        .unwrap();
    let ran = std::sync::Arc::new(AtomicBool::new(false));
    let saw = ran.clone();
    let started = Instant::now();
    let outcome = terminal_approval_within(Duration::from_millis(60), move |_| {
        saw.store(true, Ordering::Release);
        ApprovalOutcome::AllowOnce
    })
    .await;
    assert_eq!(outcome, ApprovalOutcome::Deny);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    assert!(!ran.load(Ordering::Acquire));
    assert_eq!(holder.await.unwrap(), ApprovalOutcome::Deny);
}

#[tokio::test]
async fn a_cancelled_request_is_denied_while_still_queued_and_frees_its_place() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    let (holder, wait_held) = hold_the_terminal(600).await;
    tokio::time::timeout(Duration::from_secs(10), wait_held)
        .await
        .unwrap()
        .unwrap();
    let ran = std::sync::Arc::new(AtomicBool::new(false));
    let saw = ran.clone();
    let queued = terminal_approval(move |_| {
        saw.store(true, Ordering::Release);
        ApprovalOutcome::AllowOnce
    });
    let started = Instant::now();
    let outcome = deny_when_cancelled(tokio::time::sleep(Duration::from_millis(60)), queued).await;
    assert_eq!(outcome, ApprovalOutcome::Deny);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(holder.await.unwrap(), ApprovalOutcome::Deny);
    let next = terminal_approval(|_| ApprovalOutcome::AllowOnce).await;
    assert_eq!(
        next,
        ApprovalOutcome::AllowOnce,
        "the cancelled request did not keep the terminal"
    );
    assert!(!ran.load(Ordering::Acquire));
}

#[tokio::test]
async fn an_approval_that_finishes_before_cancellation_keeps_its_answer() {
    let outcome = deny_when_cancelled(std::future::pending(), async { ApprovalOutcome::AllowOnce }).await;
    assert_eq!(outcome, ApprovalOutcome::AllowOnce);
}

#[test]
fn the_tool_flag_repeats_and_needs_a_token_to_issue() {
    let parsed = Args::try_parse_from([
        "bookie-agentd",
        "--policy",
        "p.toml",
        "--issue-token",
        "agent",
        "--tool",
        "routine_source",
        "--tool",
        "schema_context",
    ])
    .unwrap();
    assert_eq!(parsed.tools, ["routine_source", "schema_context"]);
    assert!(Args::try_parse_from(["bookie-agentd", "--policy", "p.toml", "--tool", "list_tables"]).is_err());
}
