//! Headless BookiE agent process. Serves MCP on demand over stdio with a
//! required policy file and no GTK dependency.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bookie_agentd::DaemonProvider;
use bookie_core::DriverRegistry;
use bookie_mcp::{McpBridge, TokenPermissions, TokenStore, serve_stdio};
use bookie_policy::{ApprovalOutcome, ApprovalRequest, ApprovalSink, AuditState, DenyApprovalSink, load_from_path};
use bookie_storage::{AuditJournal, SavedConnection, load_connections};
use clap::{Parser, ValueEnum};
use uuid::Uuid;

mod tty_answer;

#[derive(Parser, Debug)]
#[command(name = "bookie-agentd", about = "Headless BookiE MCP agent daemon")]
struct Args {
    /// Path to policy.toml (required).
    #[arg(long)]
    policy: PathBuf,

    /// Approval strategy when policy requires approval.
    #[arg(long, default_value = "deny")]
    approval: ApprovalMode,

    /// Issue a new token and print it, then exit.
    #[arg(long)]
    issue_token: Option<String>,

    #[arg(long, requires = "issue_token")]
    connection: Vec<Uuid>,

    #[arg(long = "tool", requires = "issue_token")]
    tools: Vec<String>,

    /// Scope for --issue-token. Defaults to the least privilege that is useful.
    #[arg(long, requires = "issue_token", default_value = "read-only")]
    permissions: TokenScope,

    /// Expire the issued token after this many days.
    #[arg(long, requires = "issue_token")]
    expires_days: Option<u32>,

    /// Write the issued token here with owner-only permissions instead of to stdout.
    #[arg(long, requires = "issue_token")]
    token_file: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum TokenScope {
    ReadOnly,
    ReadWrite,
}

impl From<TokenScope> for TokenPermissions {
    fn from(scope: TokenScope) -> Self {
        match scope {
            TokenScope::ReadOnly => TokenPermissions::ReadOnly,
            TokenScope::ReadWrite => TokenPermissions::ReadWrite,
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
enum ApprovalMode {
    Deny,
    Tty,
}

struct TtyApprovalSink;

struct CancelApprovalReader(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for CancelApprovalReader {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Release);
    }
}

/// How long a human has to answer one prompt before it is treated as a
/// denial. Without this, one unanswered prompt would block every later
/// tool call forever, since the stdio server processes requests serially.
const APPROVAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
const APPROVAL_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

#[async_trait]
impl ApprovalSink for TtyApprovalSink {
    async fn request(&self, req: ApprovalRequest) -> ApprovalOutcome {
        let cancellation = req.cancellation.clone();
        let nonce = Uuid::new_v4().to_string();
        let prompt = format!(
            "\n[bookie-agentd] approval required\n  connection: {}\n  rule: {}\n  reason: {}\n  sql: {}\nType 'approve {}' to approve, or press Enter to deny: ",
            sanitize_for_terminal(&req.connection_name),
            sanitize_for_terminal(&req.rule),
            sanitize_for_terminal(&req.reason),
            sanitize_for_terminal(&req.sql),
            nonce,
        );
        let queued_cancel = cancellation.clone();
        let approval = terminal_approval(move |cancelled| {
            ask_on_controlling_terminal(&prompt, &nonce, std::time::Instant::now() + APPROVAL_TIMEOUT, || {
                cancelled.load(std::sync::atomic::Ordering::Acquire)
                    || cancellation.as_ref().is_some_and(|token| token.is_cancelled())
            })
        });
        deny_when_cancelled(
            async move {
                match queued_cancel {
                    Some(token) => token.cancelled().await,
                    None => std::future::pending().await,
                }
            },
            approval,
        )
        .await
    }
}

async fn deny_when_cancelled<F>(cancelled: impl std::future::Future<Output = ()>, approval: F) -> ApprovalOutcome
where
    F: std::future::Future<Output = ApprovalOutcome>,
{
    tokio::select! {
        outcome = approval => outcome,
        () = cancelled => ApprovalOutcome::Deny,
    }
}

async fn terminal_approval<F>(work: F) -> ApprovalOutcome
where
    F: FnOnce(std::sync::Arc<std::sync::atomic::AtomicBool>) -> ApprovalOutcome + Send + 'static,
{
    terminal_approval_within(APPROVAL_TIMEOUT, work).await
}

async fn terminal_approval_within<F>(queue_wait: std::time::Duration, work: F) -> ApprovalOutcome
where
    F: FnOnce(std::sync::Arc<std::sync::atomic::AtomicBool>) -> ApprovalOutcome + Send + 'static,
{
    static TERMINAL: std::sync::OnceLock<std::sync::Arc<tokio::sync::Mutex<()>>> = std::sync::OnceLock::new();
    let terminal = TERMINAL
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    let Ok(lease) = tokio::time::timeout(queue_wait, terminal.lock_owned()).await else {
        tracing::warn!(
            seconds = queue_wait.as_secs(),
            "another approval prompt held the terminal; denying"
        );
        return ApprovalOutcome::Deny;
    };
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let _cancel_reader = CancelApprovalReader(cancelled.clone());
    approval_with_timeout(APPROVAL_TIMEOUT + APPROVAL_GRACE, move || {
        let _lease = lease;
        work(cancelled)
    })
    .await
}

async fn approval_with_timeout<F>(timeout: std::time::Duration, work: F) -> ApprovalOutcome
where
    F: FnOnce() -> ApprovalOutcome + Send + 'static,
{
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(work)).await {
        Ok(joined) => joined.unwrap_or(ApprovalOutcome::Deny),
        Err(_) => {
            tracing::warn!(
                seconds = timeout.as_secs(),
                "approval prompt still running past its deadline; denying"
            );
            ApprovalOutcome::Deny
        }
    }
}

/// Strips control characters (including ESC) from operator-facing text
/// before it is written to the terminal. `req.sql` is attacker-controlled:
/// an agent could otherwise embed ANSI escapes and carriage returns to
/// erase the displayed rule and reason and redraw a fake prompt, so the
/// human approves something other than what they read.
fn sanitize_for_terminal(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || is_invisible_format(c) {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

fn is_invisible_format(c: char) -> bool {
    matches!(
        c,
        '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
    )
}

fn ask_on_controlling_terminal(
    prompt: &str,
    nonce: &str,
    deadline: std::time::Instant,
    cancelled: impl Fn() -> bool,
) -> ApprovalOutcome {
    use std::io::Write;

    let terminal = match std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty") {
        Ok(terminal) => terminal,
        Err(error) => {
            tracing::warn!(%error, "no controlling terminal for approval; denying");
            return ApprovalOutcome::Deny;
        }
    };
    if cancelled() {
        return ApprovalOutcome::Deny;
    }
    tty_answer::discard_pending_input(&terminal);
    let mut writer = match terminal.try_clone() {
        Ok(writer) => writer,
        Err(error) => {
            tracing::warn!(%error, "controlling terminal is not writable; denying");
            return ApprovalOutcome::Deny;
        }
    };
    if writer.write_all(prompt.as_bytes()).is_err() || writer.flush().is_err() {
        return ApprovalOutcome::Deny;
    }

    let mut terminal = terminal;
    match tty_answer::read_answer(&mut terminal, deadline, &cancelled, nonce) {
        Some(outcome) => outcome,
        None => {
            let message: &[u8] = if cancelled() {
                b"\n[bookie-agentd] approval cancelled; denied\n"
            } else {
                b"\n[bookie-agentd] approval timed out; denied\n"
            };
            tracing::warn!("approval prompt unanswered; denying");
            let _ = writer.write_all(message);
            ApprovalOutcome::Deny
        }
    }
}

fn outcome_for_answer(answer: &str, nonce: &str) -> ApprovalOutcome {
    if answer.trim() == format!("approve {nonce}") {
        ApprovalOutcome::AllowOnce
    } else {
        ApprovalOutcome::Deny
    }
}

fn write_token_file(path: &std::path::Path, plaintext: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut handle = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !handle
        .metadata()
        .map_err(|e| format!("{}: {e}", path.display()))?
        .is_file()
    {
        return Err(format!(
            "{}: the token file path must be a regular file",
            path.display()
        ));
    }
    handle
        .set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    handle.set_len(0).map_err(|e| format!("{}: {e}", path.display()))?;
    handle
        .write_all(plaintext.as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    handle.sync_all().map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

fn validate_token_connections(requested: &[Uuid], saved: &[SavedConnection]) -> Result<(), String> {
    if requested.is_empty() {
        return Err("--issue-token requires at least one --connection UUID".into());
    }
    let saved_ids: HashSet<Uuid> = saved.iter().map(|connection| connection.id).collect();
    if let Some(unknown) = requested.iter().find(|id| !saved_ids.contains(id)) {
        return Err(format!("connection {unknown} not found"));
    }
    Ok(())
}

fn emit_token(plain: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{plain}")?;
    out.flush()
}

fn build_registry() -> DriverRegistry {
    drivers_registry::default_registry()
}

async fn daemon_provider(
    policy: Arc<bookie_policy::PolicyConfig>,
    audit: Arc<dyn bookie_policy::AuditSink>,
    approval: Arc<dyn bookie_policy::ApprovalSink>,
) -> DaemonProvider {
    let provider = DaemonProvider::new(
        Arc::new(build_registry()),
        policy,
        audit,
        Arc::new(AuditState::new()),
        approval,
    );
    match bookie_transport::system_openssh(Arc::new(bookie_ssh::openssh::UnattendedPrompter)).await {
        Ok(openssh) => provider.with_system_openssh(openssh),
        Err(error) => {
            tracing::info!(error = %error, "system OpenSSH connections are unavailable");
            provider
        }
    }
}

async fn issue_token_command(name: String, args: Args, tokens: &TokenStore) -> Result<(), Box<dyn std::error::Error>> {
    let saved = load_connections().await?;
    validate_token_connections(&args.connection, &saved)?;
    let expires_at = match args.expires_days {
        Some(days) => Some(
            chrono::Utc::now()
                .checked_add_signed(chrono::TimeDelta::days(i64::from(days)))
                .ok_or("--expires-days is too large")?,
        ),
        None => None,
    };
    let tool_allowlist = (!args.tools.is_empty()).then_some(args.tools);
    let (_meta, plain) = tokens.issue_with_tools(
        name,
        args.permissions.into(),
        args.connection,
        tool_allowlist,
        expires_at,
    )?;
    match args.token_file {
        Some(path) => write_token_file(&path, &plain)?,
        None => emit_token(&plain)?,
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookie_transport::install_crypto_provider();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .log_internal_errors(false)
        .init();
    bookie_policy::withhold_panic_messages();
    if let Err(error) = bookie_storage::migrate_legacy_state() {
        tracing::error!(%error, "legacy tablepro state could not be copied; it was left untouched");
    }

    let mut args = Args::parse();
    if !args.policy.exists() {
        tracing::error!(path = %args.policy.display(), "policy file not found");
        std::process::exit(2);
    }
    let policy = Arc::new(load_from_path(&args.policy).map_err(|e| e.to_string())?);

    let tokens = Arc::new(TokenStore::open_default().map_err(|e| e.to_string())?);
    if let Some(name) = args.issue_token.take() {
        return issue_token_command(name, args, &tokens).await;
    }

    let approval: Arc<dyn bookie_policy::ApprovalSink> = match args.approval {
        ApprovalMode::Deny => Arc::new(DenyApprovalSink),
        ApprovalMode::Tty => Arc::new(TtyApprovalSink),
    };
    let journal =
        AuditJournal::open_default().map_err(|error| format!("required audit journal unavailable: {error}"))?;
    if journal.recovery().has_unresolved_global_writes() {
        return Err(format!(
            "refusing to start with {} unresolved audit operation(s)",
            journal.recovery().unresolved_operation_ids().len()
        )
        .into());
    }
    let audit: Arc<dyn bookie_policy::AuditSink> = Arc::new(journal);

    let provider = Arc::new(daemon_provider(policy, audit, approval).await);
    let bridge = Arc::new(McpBridge::new(provider, tokens));

    serve_stdio(bridge).await?;
    Ok(())
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
