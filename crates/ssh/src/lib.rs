use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UnixListener};
use tokio_util::sync::CancellationToken;

use russh::ChannelMsg;
use russh::client::{self, Config, Handle};
use russh::keys::ssh_key::{HashAlg, PublicKey};
use russh::keys::{PrivateKeyWithHashAlg, load_secret_key};

use crate::openssh::Prompter;

mod handshake;
mod known_hosts;
use handshake::{HandshakeWaitError, HostKeyPromptEvent, wait_for_handshake};
use known_hosts::HostKeyOutcome;
pub mod openssh;
pub use known_hosts::default_known_hosts_path;

const LOCAL_BIND_HOST: &str = "127.0.0.1";

#[derive(Debug, Clone)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownHostKey {
    Learn,
    Refuse,
}

#[derive(Debug, Clone)]
pub enum SshAuth {
    Password {
        password: SecretString,
    },
    PrivateKey {
        path: PathBuf,
        passphrase: Option<SecretString>,
    },
    Agent,
    AgentIdentity {
        public_key: PathBuf,
    },
}

#[derive(Debug, Error)]
pub enum SshError {
    #[error("connect: {0}")]
    Connect(String),
    #[error("authentication failed")]
    Auth,
    #[error(
        "the server closed the connection after refusing {refused} of the {total} keys held by the SSH agent; its limit on authentication attempts is probably lower than that. Remove keys you do not need from the agent (ssh-add -d) or use a key file"
    )]
    AgentKeyLimit { refused: usize, total: usize },
    #[error(
        "the server accepts only keyboard-interactive authentication, which this client does not support; use a password or a key instead"
    )]
    KeyboardInteractiveUnsupported,
    #[error("SSH jump chain requires at least one hop")]
    EmptyChain,
    #[error("read private key {path}: {source}")]
    Key {
        path: PathBuf,
        #[source]
        source: russh::keys::Error,
    },
    #[error("local bind: {0}")]
    Bind(#[source] std::io::Error),
    #[error(
        "host key for {host}:{port} does not match {known_hosts}: stored fingerprint differs (line {line}). \
         If the server was reinstalled, remove the old line; otherwise this may indicate a man-in-the-middle attack. \
         New fingerprint: {new_fingerprint}"
    )]
    HostKeyMismatch {
        host: String,
        port: u16,
        new_fingerprint: String,
        line: usize,
        known_hosts: PathBuf,
    },
    #[error(
        "host key for {host}:{port} is not in {known_hosts}, and this connection does not add new keys. \
         Running ssh will not help: it writes to its own known_hosts file, not this one. Connect once from \
         BookiE with a setting that trusts new keys, or add the line to {known_hosts} yourself. \
         Fingerprint: {fingerprint}"
    )]
    UnknownHostKey {
        host: String,
        port: u16,
        fingerprint: String,
        known_hosts: PathBuf,
    },
    #[error("known_hosts: {0}")]
    KnownHosts(String),
    #[error("ssh-agent: {0}")]
    Agent(String),
    #[error("ssh: {0}")]
    Ssh(#[from] russh::Error),
    #[error("{stage} to {host}:{port} did not complete within {seconds} seconds")]
    Timeout {
        stage: &'static str,
        host: String,
        port: u16,
        seconds: u64,
    },
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const AUTH_TIMEOUT: Duration = Duration::from_secs(20);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);

fn client_config() -> Config {
    Config {
        nodelay: true,
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        ..Default::default()
    }
}

fn timeout_error(stage: &'static str, cfg: &SshConfig, limit: Duration) -> SshError {
    SshError::Timeout {
        stage,
        host: cfg.host.clone(),
        port: cfg.port,
        seconds: limit.as_secs(),
    }
}

/// Local listener that forwards through one or more SSH sessions.
///
/// The final hop binds either a loopback TCP port or a Unix socket in a
/// private directory. A socket lets a driver dial the tunnel while TLS
/// still verifies the original service hostname.
///
/// For a jump chain, intermediate hops stay alive as nested
/// [`SshTunnel`] values so each local forward remains reachable.
pub struct SshTunnel {
    local_port: u16,
    socket_dir: Option<LocalSocketDir>,
    cancel: CancellationToken,
    session: Arc<Handle<ClientHandler>>,
    _task: tokio::task::JoinHandle<()>,
    /// Intermediate hop tunnels (dropped after this forwarder cancels).
    _upstream: Vec<SshTunnel>,
}

#[derive(Debug, Clone)]
enum LocalBind {
    Tcp,
    Socket { name: String },
}

struct LocalSocketDir {
    path: PathBuf,
}

impl LocalSocketDir {
    fn create(socket_name_len: usize) -> Result<Self, SshError> {
        let preferred = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let mut last_error = None;
        for base in [preferred, PathBuf::from("/tmp")] {
            for _ in 0..8 {
                let unique = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_nanos())
                    .unwrap_or_default();
                let path = base.join(format!("tablepro-ssh-{}-{unique}", std::process::id()));
                if !socket_dir_path_fits(path.as_os_str().len(), socket_name_len) {
                    break;
                }
                match std::fs::DirBuilder::new().recursive(false).mode(0o700).create(&path) {
                    Ok(()) => return Ok(Self { path }),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => last_error = Some(error),
                    Err(error) => {
                        last_error = Some(error);
                        break;
                    }
                }
            }
        }
        Err(SshError::Bind(last_error.unwrap_or_else(|| {
            std::io::Error::other("no private runtime directory can fit the forwarded socket path")
        })))
    }
}

fn socket_dir_path_fits(candidate_path_len: usize, socket_name_len: usize) -> bool {
    candidate_path_len + 1 + socket_name_len <= MAX_SOCKET_PATH_LEN
}

impl Drop for LocalSocketDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl SshTunnel {
    /// Single-hop convenience: tunnel through `cfg` to `remote_host:remote_port`.
    pub async fn open(
        cfg: SshConfig,
        remote_host: String,
        remote_port: u16,
        unknown: UnknownHostKey,
    ) -> Result<Self, SshError> {
        Self::open_chain(std::slice::from_ref(&cfg), remote_host, remote_port, unknown).await
    }

    /// Multi-hop tunnel. `hops[0]` is the first bastion (TCP connect
    /// goes there directly); `hops[n-1]` is the last jump before the
    /// database. Nested local forwards: hop0 → hop1:22 → … → remote.
    pub async fn open_chain(
        hops: &[SshConfig],
        remote_host: String,
        remote_port: u16,
        unknown: UnknownHostKey,
    ) -> Result<Self, SshError> {
        Self::open_chain_with(hops, remote_host, remote_port, LocalBind::Tcp, unknown, None).await
    }

    pub async fn open_chain_prompted(
        hops: &[SshConfig],
        remote_host: String,
        remote_port: u16,
        unknown: UnknownHostKey,
        prompter: Arc<dyn Prompter>,
    ) -> Result<Self, SshError> {
        Self::open_chain_with(hops, remote_host, remote_port, LocalBind::Tcp, unknown, Some(prompter)).await
    }

    /// Multi-hop tunnel whose final local endpoint is a Unix socket
    /// named `socket_name` inside a private directory. A driver that
    /// reports a forwarded socket name dials that socket and keeps
    /// verifying TLS against the original service hostname.
    pub async fn open_chain_socket(
        hops: &[SshConfig],
        remote_host: String,
        remote_port: u16,
        socket_name: &str,
        unknown: UnknownHostKey,
    ) -> Result<Self, SshError> {
        Self::open_chain_with(
            hops,
            remote_host,
            remote_port,
            LocalBind::Socket {
                name: socket_name.to_string(),
            },
            unknown,
            None,
        )
        .await
    }

    pub async fn open_chain_socket_prompted(
        hops: &[SshConfig],
        remote_host: String,
        remote_port: u16,
        socket_name: &str,
        unknown: UnknownHostKey,
        prompter: Arc<dyn Prompter>,
    ) -> Result<Self, SshError> {
        Self::open_chain_with(
            hops,
            remote_host,
            remote_port,
            LocalBind::Socket {
                name: socket_name.to_string(),
            },
            unknown,
            Some(prompter),
        )
        .await
    }

    async fn open_chain_with(
        hops: &[SshConfig],
        remote_host: String,
        remote_port: u16,
        bind: LocalBind,
        unknown: UnknownHostKey,
        prompter: Option<Arc<dyn Prompter>>,
    ) -> Result<Self, SshError> {
        if hops.is_empty() {
            return Err(SshError::EmptyChain);
        }

        let mut upstream: Vec<SshTunnel> = Vec::new();
        let mut tcp_host = hops[0].host.clone();
        let mut tcp_port = hops[0].port;

        for (i, hop) in hops.iter().enumerate() {
            let is_last = i + 1 == hops.len();
            let (fwd_host, fwd_port) = if is_last {
                (remote_host.as_str(), remote_port)
            } else {
                (hops[i + 1].host.as_str(), hops[i + 1].port)
            };

            let hop_bind = if is_last { bind.clone() } else { LocalBind::Tcp };
            let mut tunnel = open_single(
                hop,
                &tcp_host,
                tcp_port,
                fwd_host.to_string(),
                fwd_port,
                hop_bind,
                unknown,
                prompter.clone(),
            )
            .await?;

            if is_last {
                tunnel._upstream = upstream;
                return Ok(tunnel);
            }

            tcp_host = tunnel.local_host().to_string();
            tcp_port = tunnel.local_port();
            upstream.push(tunnel);
        }

        Err(SshError::EmptyChain)
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    pub fn local_host(&self) -> &'static str {
        LOCAL_BIND_HOST
    }

    /// Directory holding the forwarded Unix socket, when the tunnel was
    /// opened with [`SshTunnel::open_chain_socket`].
    pub fn socket_dir(&self) -> Option<&Path> {
        self.socket_dir.as_ref().map(|dir| dir.path.as_path())
    }

    /// True once the SSH session has ended, through cancellation, a
    /// remote disconnect, or a keepalive timeout against an unresponsive
    /// hop. A caller can poll this to reconnect a lost tunnel instead of
    /// waiting for a database driver's own timeout to notice.
    pub fn is_closed(&self) -> bool {
        self.session.is_closed()
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct ClientHandler {
    target_host: String,
    target_port: u16,
    known_hosts_path: PathBuf,
    unknown: UnknownHostKey,
    prompter: Option<Arc<dyn Prompter>>,
    prompt_events: tokio::sync::mpsc::UnboundedSender<HostKeyPromptEvent>,
    outcome: Arc<Mutex<Option<known_hosts::HostKeyOutcome>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, presented: &russh::keys::PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } = presented else {
            if let Ok(mut slot) = self.outcome.lock() {
                *slot = Some(known_hosts::HostKeyOutcome::KnownHostsIo(
                    "SSH host certificates require the OpenSSH transport".into(),
                ));
            }
            return Ok(false);
        };
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        let outcome = match &self.prompter {
            Some(prompter) => {
                known_hosts::verify_or_prompt(
                    &self.target_host,
                    self.target_port,
                    key,
                    &self.known_hosts_path,
                    &fingerprint,
                    prompter.as_ref(),
                    &self.prompt_events,
                )
                .await
            }
            None => known_hosts::verify_or_learn(
                &self.target_host,
                self.target_port,
                key,
                &self.known_hosts_path,
                &fingerprint,
                self.unknown,
            ),
        };
        let allow = matches!(
            outcome,
            known_hosts::HostKeyOutcome::Trusted | known_hosts::HostKeyOutcome::LearnedNew { .. }
        );
        if let Ok(mut slot) = self.outcome.lock() {
            *slot = Some(outcome);
        }
        Ok(allow)
    }
}

/// Open one hop: TCP to `(tcp_host, tcp_port)`, verify host key as
/// `cfg.host:cfg.port`, forward local listeners to `fwd_host:fwd_port`.
#[allow(clippy::too_many_arguments)]
async fn open_single(
    cfg: &SshConfig,
    tcp_host: &str,
    tcp_port: u16,
    fwd_host: String,
    fwd_port: u16,
    bind: LocalBind,
    unknown: UnknownHostKey,
    prompter: Option<Arc<dyn Prompter>>,
) -> Result<SshTunnel, SshError> {
    let session = Arc::new(connect_and_auth(cfg, tcp_host, tcp_port, unknown, prompter).await?);
    let (listener, local_port, socket_dir) = bind_local(bind).await?;

    tracing::info!(
        ssh_host = %cfg.host,
        ssh_port = cfg.port,
        tcp_host,
        tcp_port,
        local_port,
        forwarded_socket = socket_dir.is_some(),
        remote_host = %fwd_host,
        remote_port = fwd_port,
        "ssh tunnel listening"
    );

    let cancel = CancellationToken::new();
    let task = tokio::spawn(forwarder_loop(
        listener,
        session.clone(),
        fwd_host,
        fwd_port,
        cancel.clone(),
    ));

    Ok(SshTunnel {
        local_port,
        socket_dir,
        cancel,
        session,
        _task: task,
        _upstream: Vec::new(),
    })
}

const MAX_SOCKET_PATH_LEN: usize = 100;

async fn bind_local(bind: LocalBind) -> Result<(LocalListener, u16, Option<LocalSocketDir>), SshError> {
    match bind {
        LocalBind::Tcp => {
            let listener = TcpListener::bind((LOCAL_BIND_HOST, 0)).await.map_err(SshError::Bind)?;
            let local_port = listener.local_addr().map_err(SshError::Bind)?.port();
            Ok((LocalListener::Tcp(listener), local_port, None))
        }
        LocalBind::Socket { name } => {
            let directory = LocalSocketDir::create(name.len())?;
            let path = directory.path.join(&name);
            check_socket_path_length(path.as_os_str().len())?;
            let listener = UnixListener::bind(&path).map_err(|error| {
                SshError::Bind(std::io::Error::new(
                    error.kind(),
                    format!("bind {}: {error}", path.display()),
                ))
            })?;
            Ok((LocalListener::Socket(listener), 0, Some(directory)))
        }
    }
}

fn check_socket_path_length(len: usize) -> Result<(), SshError> {
    if len > MAX_SOCKET_PATH_LEN {
        return Err(SshError::Bind(std::io::Error::other(format!(
            "forwarded socket path is too long: {len} bytes"
        ))));
    }
    Ok(())
}

enum LocalListener {
    Tcp(TcpListener),
    Socket(UnixListener),
}

enum LocalStream {
    Tcp(TcpStream),
    Socket(tokio::net::UnixStream),
}

impl LocalListener {
    async fn accept(&self) -> std::io::Result<(LocalStream, String, u32)> {
        match self {
            Self::Tcp(listener) => {
                let (stream, peer) = listener.accept().await?;
                Ok((LocalStream::Tcp(stream), peer.ip().to_string(), u32::from(peer.port())))
            }
            Self::Socket(listener) => {
                let (stream, _) = listener.accept().await?;
                Ok((LocalStream::Socket(stream), LOCAL_BIND_HOST.to_string(), 0))
            }
        }
    }
}

async fn connect_and_auth(
    cfg: &SshConfig,
    tcp_host: &str,
    tcp_port: u16,
    unknown: UnknownHostKey,
    prompter: Option<Arc<dyn Prompter>>,
) -> Result<Handle<ClientHandler>, SshError> {
    let known_hosts_path = default_known_hosts_path()
        .ok_or_else(|| SshError::KnownHosts("neither XDG_CONFIG_HOME nor HOME is set".into()))?;
    let outcome = Arc::new(Mutex::new(None));
    let (prompt_events, prompt_event_receiver) = tokio::sync::mpsc::unbounded_channel();
    let handler = ClientHandler {
        target_host: cfg.host.clone(),
        target_port: cfg.port,
        known_hosts_path: known_hosts_path.clone(),
        unknown,
        prompter,
        prompt_events,
        outcome: outcome.clone(),
    };

    let config = Arc::new(client_config());
    let connecting = client::connect(config, (tcp_host, tcp_port), handler);
    let mut session = match wait_for_handshake(connecting, prompt_event_receiver, CONNECT_TIMEOUT).await {
        Ok(s) => s,
        Err(HandshakeWaitError::Handshake(e)) => {
            return Err(map_connect_error(e, &cfg.host, cfg.port, &known_hosts_path, &outcome));
        }
        Err(HandshakeWaitError::HostKeyDeclined(fingerprint)) => {
            return Err(unknown_host_key(&cfg.host, cfg.port, fingerprint, &known_hosts_path));
        }
        Err(HandshakeWaitError::Timeout) => return Err(timeout_error("ssh handshake", cfg, CONNECT_TIMEOUT)),
    };

    match outcome.lock().unwrap_or_else(PoisonError::into_inner).clone() {
        Some(HostKeyOutcome::LearnedNew { fingerprint }) => tracing::info!(
            host = %cfg.host,
            port = cfg.port,
            fingerprint = %fingerprint,
            "ssh: learned new host key (TOFU)",
        ),
        Some(HostKeyOutcome::Trusted) => tracing::debug!(host = %cfg.host, "ssh: host key matches known_hosts"),
        Some(HostKeyOutcome::KnownHostsIo(e)) => return Err(SshError::KnownHosts(e)),
        Some(HostKeyOutcome::Unknown { fingerprint }) => {
            return Err(unknown_host_key(&cfg.host, cfg.port, fingerprint, &known_hosts_path));
        }
        Some(HostKeyOutcome::Changed { fingerprint, line }) => {
            return Err(SshError::HostKeyMismatch {
                host: cfg.host.clone(),
                port: cfg.port,
                new_fingerprint: fingerprint,
                line,
                known_hosts: known_hosts_path.clone(),
            });
        }
        None => {}
    }

    let authenticating = async {
        match &cfg.auth {
            SshAuth::Password { password } => session
                .authenticate_password(&cfg.username, password.expose_secret())
                .await
                .map_err(SshError::Ssh),
            SshAuth::PrivateKey { path, passphrase } => {
                let pp = passphrase.as_ref().map(|s| s.expose_secret().to_string());
                let key = load_secret_key(path, pp.as_deref()).map_err(|e| SshError::Key {
                    path: path.clone(),
                    source: e,
                })?;
                let hash = session.best_supported_rsa_hash().await?.flatten();
                session
                    .authenticate_publickey(&cfg.username, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                    .await
                    .map_err(SshError::Ssh)
            }
            SshAuth::Agent => authenticate_with_agent(&mut session, &cfg.username, None).await,
            SshAuth::AgentIdentity { public_key } => {
                authenticate_with_agent(&mut session, &cfg.username, Some(public_key)).await
            }
        }
    };
    let auth = match tokio::time::timeout(AUTH_TIMEOUT, authenticating).await {
        Ok(result) => result?,
        Err(_) => return Err(timeout_error("ssh authentication", cfg, AUTH_TIMEOUT)),
    };

    if !auth.success() {
        return Err(auth_failure_error(&auth));
    }
    Ok(session)
}

fn auth_failure_error(auth: &client::AuthResult) -> SshError {
    if let client::AuthResult::Failure { remaining_methods, .. } = auth
        && requires_keyboard_interactive_only(remaining_methods)
    {
        return SshError::KeyboardInteractiveUnsupported;
    }
    SshError::Auth
}

fn requires_keyboard_interactive_only(remaining_methods: &russh::MethodSet) -> bool {
    !remaining_methods.is_empty()
        && remaining_methods
            .iter()
            .all(|method| matches!(method, russh::MethodKind::KeyboardInteractive))
}

async fn authenticate_with_agent(
    session: &mut Handle<ClientHandler>,
    username: &str,
    only: Option<&Path>,
) -> Result<client::AuthResult, SshError> {
    let agent_error = |error: &dyn std::fmt::Display| SshError::Agent(error.to_string());
    let mut agent = russh::keys::agent::client::AgentClient::connect_env()
        .await
        .map_err(|error| agent_error(&error))?;
    let keys: Vec<PublicKey> = agent
        .request_identities()
        .await
        .map_err(|error| agent_error(&error))?
        .into_iter()
        .filter_map(|identity| match identity {
            russh::keys::agent::AgentIdentity::PublicKey { key, .. } => Some(key),
            _ => None,
        })
        .collect();
    if keys.is_empty() {
        return Err(SshError::Agent("the agent holds no keys".into()));
    }
    let keys = match only {
        Some(path) => {
            let wanted = PublicKey::read_openssh_file(path)
                .map_err(|error| SshError::Agent(format!("cannot read the public key {}: {error}", path.display())))?;
            let chosen: Vec<PublicKey> = keys
                .into_iter()
                .filter(|key| key.key_data() == wanted.key_data())
                .collect();
            if chosen.is_empty() {
                return Err(SshError::Agent(format!(
                    "the agent does not hold the key in {}",
                    path.display()
                )));
            }
            chosen
        }
        None => keys,
    };
    let hash = session.best_supported_rsa_hash().await?.flatten();
    let total = keys.len();
    for (refused, key) in keys.into_iter().enumerate() {
        let result = match session
            .authenticate_publickey_with(username, key, hash, &mut agent)
            .await
        {
            Ok(result) => result,
            Err(_) if refused > 0 => return Err(SshError::AgentKeyLimit { refused, total }),
            Err(error) => return Err(agent_error(&error)),
        };
        if result.success() {
            return Ok(result);
        }
    }
    Err(SshError::Auth)
}

fn map_connect_error(
    err: russh::Error,
    host: &str,
    port: u16,
    known_hosts: &Path,
    outcome: &Arc<Mutex<Option<HostKeyOutcome>>>,
) -> SshError {
    // Recover a poisoned mutex so a panic during host-key verification
    // never silently downgrades a HostKeyMismatch to a generic Connect error.
    let captured = outcome.lock().unwrap_or_else(PoisonError::into_inner).clone();
    match captured {
        Some(HostKeyOutcome::Changed { fingerprint, line }) => SshError::HostKeyMismatch {
            host: host.to_string(),
            port,
            new_fingerprint: fingerprint,
            line,
            known_hosts: known_hosts.to_path_buf(),
        },
        Some(HostKeyOutcome::KnownHostsIo(e)) => SshError::KnownHosts(e),
        Some(HostKeyOutcome::Unknown { fingerprint }) => unknown_host_key(host, port, fingerprint, known_hosts),
        _ => SshError::Connect(err.to_string()),
    }
}

fn unknown_host_key(host: &str, port: u16, fingerprint: String, known_hosts: &Path) -> SshError {
    SshError::UnknownHostKey {
        host: host.to_string(),
        port,
        fingerprint,
        known_hosts: known_hosts.to_path_buf(),
    }
}

async fn forwarder_loop(
    listener: LocalListener,
    session: Arc<Handle<ClientHandler>>,
    remote_host: String,
    remote_port: u16,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::debug!("ssh tunnel cancelled");
                return;
            }
            accept = listener.accept() => match accept {
                Ok((socket, originator_host, originator_port)) => {
                    let session = session.clone();
                    let remote_host = remote_host.clone();
                    let cancel = cancel.clone();
                    tokio::spawn(async move {
                        let forwarded = match socket {
                            LocalStream::Tcp(stream) => {
                                forward_one(session, stream, originator_host, originator_port, remote_host, remote_port, cancel).await
                            }
                            LocalStream::Socket(stream) => {
                                forward_one(session, stream, originator_host, originator_port, remote_host, remote_port, cancel).await
                            }
                        };
                        if let Err(e) = forwarded {
                            tracing::warn!(error = %e, "ssh forward failed");
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!(error = %e, "ssh listener accept failed");
                    return;
                }
            },
        }
    }
}

async fn forward_one<S>(
    session: Arc<Handle<ClientHandler>>,
    socket: S,
    originator_host: String,
    originator_port: u32,
    remote_host: String,
    remote_port: u16,
    cancel: CancellationToken,
) -> Result<(), russh::Error>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let channel = session
        .channel_open_direct_tcpip(remote_host, u32::from(remote_port), originator_host, originator_port)
        .await?;

    relay(socket, channel, cancel).await;
    Ok(())
}

trait RelayChannel {
    async fn send(&mut self, bytes: &[u8]) -> bool;
    async fn send_eof(&mut self);
    async fn next_message(&mut self) -> Option<ChannelMsg>;
}

impl RelayChannel for russh::Channel<client::Msg> {
    async fn send(&mut self, bytes: &[u8]) -> bool {
        self.data(bytes).await.is_ok()
    }

    async fn send_eof(&mut self) {
        let _ = self.eof().await;
    }

    async fn next_message(&mut self) -> Option<ChannelMsg> {
        self.wait().await
    }
}

async fn relay<S, C>(mut socket: S, mut channel: C, cancel: CancellationToken)
where
    S: AsyncRead + AsyncWrite + Unpin,
    C: RelayChannel,
{
    let mut buf = vec![0u8; 65536];
    let mut local_eof = false;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                channel.send_eof().await;
                return;
            }
            r = socket.read(&mut buf), if !local_eof => match r {
                Ok(0) => {
                    local_eof = true;
                    channel.send_eof().await;
                }
                Ok(n) => {
                    if !channel.send(&buf[..n]).await {
                        return;
                    }
                }
                Err(_) => return,
            },
            msg = channel.next_message() => match msg {
                Some(ChannelMsg::Data { data }) => {
                    if socket.write_all(&data).await.is_err() {
                        return;
                    }
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                    let _ = socket.shutdown().await;
                    return;
                }
                Some(_) => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
