use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UnixListener};
use tokio_util::sync::CancellationToken;

use russh::ChannelMsg;
use russh::client::{self, Config, Handle};
use russh::keys::known_hosts::{check_known_hosts_path, known_host_keys_path, learn_known_hosts_path};
use russh::keys::ssh_key::{HashAlg, PublicKey};
use russh::keys::{PrivateKeyWithHashAlg, load_secret_key};

pub mod openssh;

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
}

#[derive(Debug, Error)]
pub enum SshError {
    #[error("connect: {0}")]
    Connect(String),
    #[error("authentication failed")]
    Auth,
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
         Connect once from BookiE or ssh to confirm fingerprint {fingerprint}"
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
        Self::open_chain_with(hops, remote_host, remote_port, LocalBind::Tcp, unknown).await
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
        )
        .await
    }

    async fn open_chain_with(
        hops: &[SshConfig],
        remote_host: String,
        remote_port: u16,
        bind: LocalBind,
        unknown: UnknownHostKey,
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
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[derive(Debug, Clone)]
enum HostKeyOutcome {
    Trusted,
    LearnedNew { fingerprint: String },
    Changed { fingerprint: String, line: usize },
    Unknown { fingerprint: String },
    KnownHostsIo(String),
}

struct ClientHandler {
    target_host: String,
    target_port: u16,
    known_hosts_path: PathBuf,
    unknown: UnknownHostKey,
    outcome: Arc<Mutex<Option<HostKeyOutcome>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, presented: &russh::keys::PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } = presented else {
            if let Ok(mut slot) = self.outcome.lock() {
                *slot = Some(HostKeyOutcome::KnownHostsIo(
                    "SSH host certificates require the OpenSSH transport".into(),
                ));
            }
            return Ok(false);
        };
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        let outcome = verify_or_learn(
            &self.target_host,
            self.target_port,
            key,
            &self.known_hosts_path,
            &fingerprint,
            self.unknown,
        );
        let allow = matches!(outcome, HostKeyOutcome::Trusted | HostKeyOutcome::LearnedNew { .. });
        if let Ok(mut slot) = self.outcome.lock() {
            *slot = Some(outcome);
        }
        Ok(allow)
    }
}

fn verify_or_learn(
    host: &str,
    port: u16,
    key: &PublicKey,
    known_hosts: &Path,
    fingerprint: &str,
    unknown: UnknownHostKey,
) -> HostKeyOutcome {
    match check_known_hosts_path(host, port, key, known_hosts) {
        Ok(true) => HostKeyOutcome::Trusted,
        Ok(false) => match known_host_keys_path(host, port, known_hosts) {
            Ok(existing) if !existing.is_empty() => HostKeyOutcome::Changed {
                fingerprint: fingerprint.to_string(),
                line: existing[0].0,
            },
            Ok(_) if unknown == UnknownHostKey::Refuse => HostKeyOutcome::Unknown {
                fingerprint: fingerprint.to_string(),
            },
            Ok(_) => match ensure_parent_dir(known_hosts).and_then(|_| {
                learn_known_hosts_path(host, port, key, known_hosts).map_err(|e| std::io::Error::other(format!("{e}")))
            }) {
                Ok(()) => HostKeyOutcome::LearnedNew {
                    fingerprint: fingerprint.to_string(),
                },
                Err(e) => HostKeyOutcome::KnownHostsIo(e.to_string()),
            },
            Err(e) => HostKeyOutcome::KnownHostsIo(e.to_string()),
        },
        Err(russh::keys::Error::KeyChanged { line }) => HostKeyOutcome::Changed {
            fingerprint: fingerprint.to_string(),
            line,
        },
        Err(e) => HostKeyOutcome::KnownHostsIo(e.to_string()),
    }
}

fn ensure_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub fn default_known_hosts_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("tablepro").join("known_hosts"))
}

/// Open one hop: TCP to `(tcp_host, tcp_port)`, verify host key as
/// `cfg.host:cfg.port`, forward local listeners to `fwd_host:fwd_port`.
async fn open_single(
    cfg: &SshConfig,
    tcp_host: &str,
    tcp_port: u16,
    fwd_host: String,
    fwd_port: u16,
    bind: LocalBind,
    unknown: UnknownHostKey,
) -> Result<SshTunnel, SshError> {
    let session = Arc::new(connect_and_auth(cfg, tcp_host, tcp_port, unknown).await?);
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
    let task = tokio::spawn(forwarder_loop(listener, session, fwd_host, fwd_port, cancel.clone()));

    Ok(SshTunnel {
        local_port,
        socket_dir,
        cancel,
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
) -> Result<Handle<ClientHandler>, SshError> {
    let known_hosts_path = default_known_hosts_path()
        .ok_or_else(|| SshError::KnownHosts("neither XDG_CONFIG_HOME nor HOME is set".into()))?;
    let outcome = Arc::new(Mutex::new(None));
    let handler = ClientHandler {
        target_host: cfg.host.clone(),
        target_port: cfg.port,
        known_hosts_path: known_hosts_path.clone(),
        unknown,
        outcome: outcome.clone(),
    };

    let config = Arc::new(Config {
        nodelay: true,
        ..Default::default()
    });
    let connecting = client::connect(config, (tcp_host, tcp_port), handler);
    let mut session = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(map_connect_error(e, &cfg.host, cfg.port, &known_hosts_path, &outcome)),
        Err(_) => return Err(timeout_error("ssh handshake", cfg, CONNECT_TIMEOUT)),
    };

    match outcome.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone() {
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
            SshAuth::Agent => authenticate_with_agent(&mut session, &cfg.username).await,
        }
    };
    let auth = match tokio::time::timeout(AUTH_TIMEOUT, authenticating).await {
        Ok(result) => result?,
        Err(_) => return Err(timeout_error("ssh authentication", cfg, AUTH_TIMEOUT)),
    };

    if !auth.success() {
        return Err(SshError::Auth);
    }
    Ok(session)
}

async fn authenticate_with_agent(
    session: &mut Handle<ClientHandler>,
    username: &str,
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
    let hash = session.best_supported_rsa_hash().await?.flatten();
    for key in keys {
        let result = session
            .authenticate_publickey_with(username, key, hash, &mut agent)
            .await
            .map_err(|error| agent_error(&error))?;
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
    let captured = outcome.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
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
mod tests {
    use super::*;

    #[test]
    fn ssh_auth_password_redacts_in_debug() {
        let auth = SshAuth::Password {
            password: SecretString::new("topsecret".to_string().into()),
        };
        let dbg = format!("{auth:?}");
        assert!(!dbg.contains("topsecret"), "password leaked in Debug: {dbg}");
    }

    #[test]
    fn empty_chain_is_rejected() {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(SshTunnel::open_chain(&[], "db".into(), 5432, UnknownHostKey::Learn));
        assert!(matches!(result, Err(SshError::EmptyChain)));
    }

    #[test]
    fn open_is_single_hop_wrapper_shape() {
        let cfg = SshConfig {
            host: "bastion.example".into(),
            port: 22,
            username: "deploy".into(),
            auth: SshAuth::Password {
                password: SecretString::new("x".to_string().into()),
            },
        };
        let hops = std::slice::from_ref(&cfg);
        assert_eq!(hops.len(), 1);
        assert_eq!(hops[0].host, "bastion.example");
    }

    const KEY_A_BASE64: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIGAdbe+Xv3hfmzwpfcGVeMHE/jfo5bmR1IgIpfuP4ypR";
    const KEY_B_BASE64: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIFvC8V+mh5lxNlOLorBehIwTS2R/nvw2ghab6N1SlSk6";
    const RSA_KEY_BASE64: &str = "AAAAB3NzaC1yc2EAAAADAQABAAABAQDNTWi6IachyABYXmaOyLJ2TyBGTwkzBdub6FHS7xEB4XyqU9ZkAaFajaYlhT+3zmoFgDHhnNxULyJtOK3nHpcRaouO9XT8IfUqmmcVbkJrgsoS/gUyHKWqqbzpr70uZXoqM+tjbBAvPt7S6kts1QsJgi+AvPod+1lpfbe9O6Az+hREcPqHIn0BznhMzU/d6DirOCTE81fWoACZm0y6hrhE4MDU17JpTMe9E5bbNKLUh65d2BXBo0MolCClns/UA5trjRboNz+28HRMnyPqIzqMCE7J9HLmpCdTD2+aEGnD2RbsVwYKPRokiyJVOHtQGZwMsEqTgnrG7T61sM1ZCG+D";

    fn parse_key(base64: &str) -> PublicKey {
        russh::keys::parse_public_key_base64(base64).expect("valid base64 public key")
    }

    #[test]
    fn verify_or_learn_creates_known_hosts_on_first_use() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("known_hosts");
        let key = parse_key(KEY_A_BASE64);
        let outcome = verify_or_learn("bastion.example.com", 22, &key, &path, "fp", UnknownHostKey::Learn);
        assert!(matches!(outcome, HostKeyOutcome::LearnedNew { .. }));
        assert!(path.exists());
    }

    #[test]
    fn a_refusing_connection_does_not_learn_an_unknown_key_but_still_trusts_a_known_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let key = parse_key(KEY_A_BASE64);

        let refused = verify_or_learn("bastion.example.com", 22, &key, &path, "fp", UnknownHostKey::Refuse);
        assert!(matches!(refused, HostKeyOutcome::Unknown { ref fingerprint } if fingerprint == "fp"));
        assert!(!path.exists(), "a refused key must not be written to known_hosts");

        let _ = verify_or_learn("bastion.example.com", 22, &key, &path, "fp", UnknownHostKey::Learn);
        let known = verify_or_learn("bastion.example.com", 22, &key, &path, "fp", UnknownHostKey::Refuse);
        assert!(matches!(known, HostKeyOutcome::Trusted));
        let other = parse_key(KEY_B_BASE64);
        let changed = verify_or_learn("bastion.example.com", 22, &other, &path, "fp_b", UnknownHostKey::Refuse);
        assert!(matches!(changed, HostKeyOutcome::Changed { .. }));
    }

    #[test]
    fn verify_or_learn_trusts_recorded_key_on_repeat() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let key = parse_key(KEY_A_BASE64);
        let _ = verify_or_learn("bastion.example.com", 22, &key, &path, "fp", UnknownHostKey::Learn);
        let outcome = verify_or_learn("bastion.example.com", 22, &key, &path, "fp", UnknownHostKey::Learn);
        assert!(matches!(outcome, HostKeyOutcome::Trusted));
    }

    #[test]
    fn verify_or_learn_detects_key_type_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let ed25519 = parse_key(KEY_A_BASE64);
        let rsa = parse_key(RSA_KEY_BASE64);
        let _ = verify_or_learn(
            "bastion.example.com",
            22,
            &ed25519,
            &path,
            "ed25519",
            UnknownHostKey::Learn,
        );
        let outcome = verify_or_learn("bastion.example.com", 22, &rsa, &path, "rsa", UnknownHostKey::Learn);
        assert!(matches!(outcome, HostKeyOutcome::Changed { fingerprint, .. } if fingerprint == "rsa"));
    }

    #[test]
    fn verify_or_learn_detects_key_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let key_a = parse_key(KEY_A_BASE64);
        let key_b = parse_key(KEY_B_BASE64);
        let _ = verify_or_learn("bastion.example.com", 22, &key_a, &path, "fp_a", UnknownHostKey::Learn);
        let outcome = verify_or_learn("bastion.example.com", 22, &key_b, &path, "fp_b", UnknownHostKey::Learn);
        match outcome {
            HostKeyOutcome::Changed { fingerprint, .. } => assert_eq!(fingerprint, "fp_b"),
            other => panic!("expected Changed, got {other:?}"),
        }
    }

    #[test]
    fn verify_or_learn_reports_io_error_when_known_hosts_path_is_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts_dir");
        std::fs::create_dir(&path).unwrap();
        let key = parse_key(KEY_A_BASE64);
        let outcome = verify_or_learn("bastion.example.com", 22, &key, &path, "fp", UnknownHostKey::Learn);
        assert!(
            matches!(outcome, HostKeyOutcome::KnownHostsIo(_)),
            "expected KnownHostsIo, got {outcome:?}"
        );
    }

    #[test]
    fn ssh_auth_private_key_redacts_passphrase_in_debug() {
        let auth = SshAuth::PrivateKey {
            path: PathBuf::from("/home/user/.ssh/id_ed25519"),
            passphrase: Some(SecretString::new("topsecret".to_string().into())),
        };
        let dbg = format!("{auth:?}");
        assert!(!dbg.contains("topsecret"), "passphrase leaked in Debug: {dbg}");
    }

    #[test]
    fn map_connect_error_prefers_a_recorded_host_key_mismatch_over_the_raw_connect_error() {
        let known_hosts = PathBuf::from("/nonexistent/known_hosts");
        let outcome = Arc::new(Mutex::new(Some(HostKeyOutcome::Changed {
            fingerprint: "new-fp".to_string(),
            line: 3,
        })));
        let err = map_connect_error(russh::Error::Disconnect, "db.example.com", 5432, &known_hosts, &outcome);
        match err {
            SshError::HostKeyMismatch {
                host,
                port,
                new_fingerprint,
                line,
                ..
            } => {
                assert_eq!(host, "db.example.com");
                assert_eq!(port, 5432);
                assert_eq!(new_fingerprint, "new-fp");
                assert_eq!(line, 3);
            }
            other => panic!("expected HostKeyMismatch, got {other:?}"),
        }
    }

    #[test]
    fn map_connect_error_falls_back_to_a_generic_connect_error_without_a_recorded_outcome() {
        let known_hosts = PathBuf::from("/nonexistent/known_hosts");
        let outcome = Arc::new(Mutex::new(None));
        let err = map_connect_error(russh::Error::Disconnect, "db.example.com", 5432, &known_hosts, &outcome);
        assert!(matches!(err, SshError::Connect(_)), "expected Connect, got {err:?}");
    }

    #[test]
    fn check_socket_path_length_accepts_exactly_the_limit() {
        assert!(check_socket_path_length(MAX_SOCKET_PATH_LEN).is_ok());
    }

    #[test]
    fn check_socket_path_length_rejects_one_byte_over_the_limit() {
        let error = check_socket_path_length(MAX_SOCKET_PATH_LEN + 1).unwrap_err();
        let SshError::Bind(error) = error else {
            panic!("expected Bind, got {error}");
        };
        assert!(
            error.to_string().contains("forwarded socket path is too long"),
            "expected the length guard's own message, got: {error}"
        );
    }

    #[test]
    fn bind_local_rejects_a_socket_name_that_makes_the_path_too_long() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(bind_local(LocalBind::Socket {
            name: "x".repeat(MAX_SOCKET_PATH_LEN + 1),
        }));
        assert!(matches!(result, Err(SshError::Bind(_))));
    }

    #[test]
    fn local_socket_directory_fits_a_short_socket_name() {
        let name = "pg.sock";
        let directory = LocalSocketDir::create(name.len()).unwrap();
        let path = directory.path.join(name);
        assert!(path.as_os_str().len() <= MAX_SOCKET_PATH_LEN);
    }

    #[test]
    fn socket_dir_path_fits_exactly_at_the_limit() {
        assert!(socket_dir_path_fits(MAX_SOCKET_PATH_LEN - 1 - 7, 7));
    }

    #[test]
    fn socket_dir_path_fits_rejects_one_byte_over_the_limit() {
        assert!(!socket_dir_path_fits(MAX_SOCKET_PATH_LEN - 7, 7));
    }

    struct FakeChannel {
        incoming: tokio::sync::mpsc::UnboundedReceiver<ChannelMsg>,
        sent: Arc<Mutex<Vec<u8>>>,
        eof_calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl RelayChannel for FakeChannel {
        async fn send(&mut self, bytes: &[u8]) -> bool {
            self.sent.lock().unwrap().extend_from_slice(bytes);
            true
        }

        async fn send_eof(&mut self) {
            self.eof_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }

        async fn next_message(&mut self) -> Option<ChannelMsg> {
            self.incoming.recv().await
        }
    }

    struct RelayHarness {
        client: tokio::io::DuplexStream,
        remote: tokio::sync::mpsc::UnboundedSender<ChannelMsg>,
        sent: Arc<Mutex<Vec<u8>>>,
        eof_calls: Arc<std::sync::atomic::AtomicUsize>,
        task: tokio::task::JoinHandle<()>,
    }

    fn start_relay() -> RelayHarness {
        let (client, server) = tokio::io::duplex(1024);
        let (remote, incoming) = tokio::sync::mpsc::unbounded_channel();
        let sent = Arc::new(Mutex::new(Vec::new()));
        let eof_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let channel = FakeChannel {
            incoming,
            sent: sent.clone(),
            eof_calls: eof_calls.clone(),
        };
        let task = tokio::spawn(relay(server, channel, CancellationToken::new()));
        RelayHarness {
            client,
            remote,
            sent,
            eof_calls,
            task,
        }
    }

    const RELAY_DEADLINE: Duration = Duration::from_secs(2);

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn relay_sends_one_eof_after_the_local_client_closes_and_does_not_spin() {
        let mut harness = start_relay();
        harness.client.write_all(b"select 1").await.unwrap();
        harness.client.shutdown().await.unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(harness.eof_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(harness.sent.lock().unwrap().as_slice(), b"select 1");
        assert!(!harness.task.is_finished());

        harness
            .remote
            .send(ChannelMsg::Data {
                data: b"row".to_vec().into(),
            })
            .unwrap();
        harness.remote.send(ChannelMsg::Close).unwrap();
        tokio::time::timeout(RELAY_DEADLINE, harness.task)
            .await
            .unwrap()
            .unwrap();

        let mut received = Vec::new();
        harness.client.read_to_end(&mut received).await.unwrap();
        assert_eq!(received, b"row");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn relay_stops_and_closes_the_local_client_when_the_remote_sends_eof() {
        let mut harness = start_relay();
        harness.remote.send(ChannelMsg::Eof).unwrap();

        tokio::time::timeout(RELAY_DEADLINE, harness.task)
            .await
            .unwrap()
            .unwrap();

        let mut received = Vec::new();
        harness.client.read_to_end(&mut received).await.unwrap();
        assert!(received.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn relay_stops_when_the_remote_channel_is_gone() {
        let harness = start_relay();
        drop(harness.remote);

        tokio::time::timeout(RELAY_DEADLINE, harness.task)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn relay_stops_when_the_local_client_is_dropped_and_the_remote_closes() {
        let harness = start_relay();
        drop(harness.client);
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(harness.eof_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        harness.remote.send(ChannelMsg::Close).unwrap();
        tokio::time::timeout(RELAY_DEADLINE, harness.task)
            .await
            .unwrap()
            .unwrap();
    }
}
