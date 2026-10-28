use secrecy::SecretString;
use std::sync::Arc;
use std::time::Duration;
use tablepro_core::Environment;
use tablepro_core::{AuthMode, ConnectOptions, Connection, DatabaseDriver, DriverError, TlsConfig, TlsMode};
use tablepro_policy::{AuditSink, AuditState, Principal};
use tablepro_ssh::{SshAuth, SshConfig, SshTunnel};
use tablepro_storage::{
    SavedConnection, SavedSshAuth, SavedSshConfig, SshClient, load_password, load_ssh_passphrase, load_ssh_password,
};
use uuid::Uuid;

mod route;
mod session_material;

pub use route::{OpenSshEnvironment, SshEnvironment, SshRoute, Tunnel, openssh_config_for, system_openssh};
pub use session_material::session_material_digest;

const DATABASE_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

mod audit;
mod local_socket;
mod tool_endpoint;

use audit::{audit_client, record_transport_attempt};

pub use tool_endpoint::open_tool_endpoint;

use local_socket::validate_local_socket;

#[derive(Clone)]
pub struct TransportAuditContext {
    pub sink: Arc<dyn AuditSink>,
    pub state: Arc<AuditState>,
    pub principal: Principal,
    pub connection_id: Uuid,
    pub connection_name: String,
    pub environment: Environment,
    pub driver_id: String,
}

#[derive(Clone)]
pub struct TransportAuditFactory {
    sink: Arc<dyn AuditSink>,
    state: Arc<AuditState>,
}

impl TransportAuditFactory {
    pub fn new(sink: Arc<dyn AuditSink>, state: Arc<AuditState>) -> Self {
        Self { sink, state }
    }

    pub fn context(
        &self,
        principal: Principal,
        connection_id: Uuid,
        connection_name: impl Into<String>,
        environment: Environment,
        driver_id: impl Into<String>,
    ) -> TransportAuditContext {
        TransportAuditContext::new(
            self.sink.clone(),
            self.state.clone(),
            principal,
            connection_id,
            connection_name,
            environment,
            driver_id,
        )
    }
}

impl TransportAuditContext {
    pub fn new(
        sink: Arc<dyn AuditSink>,
        state: Arc<AuditState>,
        principal: Principal,
        connection_id: Uuid,
        connection_name: impl Into<String>,
        environment: Environment,
        driver_id: impl Into<String>,
    ) -> Self {
        Self {
            sink,
            state,
            principal,
            connection_id,
            connection_name: connection_name.into(),
            environment,
            driver_id: driver_id.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("ssh: {0}")]
    Ssh(String),
    #[error("ssh host-key trust was refused: {0}")]
    HostKeyRefused(String),
    #[error("ssh host key changed: {0}")]
    HostKeyChanged(String),
    #[error("ssh host key is revoked: {0}")]
    HostKeyRevoked(String),
    #[error("ssh operation timed out: {0}")]
    SshTimeout(String),
    #[error("ssh operation was cancelled")]
    Cancelled,
    #[error("transport audit could not be persisted: {0}")]
    AuditWriteFailed(String),
    #[error(
        "the system OpenSSH client is not available inside the Flatpak sandbox; switch this connection to the built-in SSH client"
    )]
    SystemSshUnavailableInSandbox,
    #[error("{0}")]
    Secret(String),
    #[error("{0}")]
    Keyring(tablepro_storage::KeyringFailure),
    #[error("integrated authentication is not supported by the {0} driver")]
    IntegratedAuthUnsupported(String),
    #[error("local Unix sockets are not supported by the {0} driver")]
    LocalSocketUnsupported(String),
    #[error("a local Unix socket cannot be combined with SSH tunnelling")]
    LocalSocketWithSsh,
    #[error("TLS must be disabled for a local Unix socket")]
    LocalSocketWithTls,
    #[error("invalid local Unix socket: {0}")]
    InvalidLocalSocket(String),
    #[error("database connection timed out after {seconds} seconds")]
    DatabaseTimeout { seconds: u64 },
    #[error(transparent)]
    Driver(#[from] DriverError),
}

/// Choose the process-wide rustls crypto provider.
///
/// The static drivers pull in both `ring` (MySQL, SQL Server, MongoDB) and
/// `aws-lc-rs` (ClickHouse), which leaves rustls unable to pick a default on
/// its own. Any library that builds a `ClientConfig` without naming a provider
/// then panics at connect time. Every composition root, including test
/// binaries that link more than one driver, must call this before connecting.
/// Calling it more than once is harmless: the first call wins.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

pub fn tls_config(mode: TlsMode) -> TlsConfig {
    TlsConfig {
        mode,
        ..Default::default()
    }
}

pub async fn connect_options_for(saved: &SavedConnection) -> Result<ConnectOptions, TransportError> {
    let password = if loads_database_password(saved) {
        load_password(saved.id)
            .await
            .map_err(|error| secret_error("load database password", error))?
            .unwrap_or_else(|| SecretString::new(String::new().into()))
    } else {
        SecretString::new(String::new().into())
    };
    Ok(ConnectOptions {
        host: saved.host.clone(),
        port: saved.port,
        database: saved.database.clone(),
        username: saved.username.clone(),
        password,
        tls: TlsConfig {
            mode: saved.effective_tls_mode(),
            root_cert: saved.tls_root_cert.clone(),
            client_cert: saved.tls_client_cert.clone(),
            client_key: saved.tls_client_key.clone(),
            ..Default::default()
        },
        auth_mode: saved.auth_mode,
        service_endpoint: None,
        local_socket_dir: saved.socket_dir.clone(),
        local_socket_path: saved.socket_path.clone(),
        forwarded_socket_dir: None,
        application_name: None,
        connect_timeout_secs: saved.connect_timeout_secs,
        read_only: saved.read_only,
        auth_source: saved.auth_source.clone(),
    })
}

pub async fn saved_ssh_route(saved: &SavedConnection) -> Result<Option<SshRoute>, TransportError> {
    saved_ssh_route_with_sandbox_override(saved, None).await
}

async fn saved_ssh_route_with_sandbox_override(
    saved: &SavedConnection,
    sandboxed: Option<bool>,
) -> Result<Option<SshRoute>, TransportError> {
    let Some(ssh) = &saved.ssh else {
        return Ok(None);
    };
    match ssh.client {
        SshClient::Builtin => resolve_saved_ssh_chain(saved.id, ssh)
            .await
            .map(|hops| Some(SshRoute::Builtin(hops))),
        SshClient::OpenSsh => {
            let config = match sandboxed {
                Some(sandboxed) => route::openssh_config_in_sandbox(saved.id, ssh, sandboxed).await,
                None => route::openssh_config(saved.id, ssh).await,
            }?;
            Ok(Some(SshRoute::OpenSsh(config)))
        }
    }
}

pub async fn establish(
    driver: &dyn DatabaseDriver,
    opts: ConnectOptions,
    ssh: Option<SshRoute>,
    environment: &SshEnvironment,
) -> Result<(Box<dyn Connection>, Option<Tunnel>), TransportError> {
    establish_with_cancellation(
        driver,
        opts,
        ssh,
        environment,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
}

pub async fn establish_with_cancellation(
    driver: &dyn DatabaseDriver,
    opts: ConnectOptions,
    ssh: Option<SshRoute>,
    environment: &SshEnvironment,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<(Box<dyn Connection>, Option<Tunnel>), TransportError> {
    let client = audit_client(ssh.as_ref());
    let started = std::time::Instant::now();
    let result = tokio::select! {
        biased;
        () = cancellation.cancelled() => Err(TransportError::Cancelled),
        result = establish_inner(driver, opts, ssh, environment, cancellation.clone()) => result,
    };
    if let Err(error) = record_transport_attempt(environment, client, started, &result).await {
        drop(result);
        return Err(error);
    }
    result
}

async fn establish_inner(
    driver: &dyn DatabaseDriver,
    mut opts: ConnectOptions,
    ssh: Option<SshRoute>,
    environment: &SshEnvironment,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<(Box<dyn Connection>, Option<Tunnel>), TransportError> {
    check_auth_mode(opts.auth_mode, driver.supports_integrated_auth(), driver.display_name())?;
    validate_client_tls_auth(driver, &opts)?;
    validate_local_socket(&opts, driver, ssh.is_some())?;
    opts.forwarded_socket_dir = None;
    let tunnel = match ssh {
        Some(route) => Some(open_tunnel(driver, &mut opts, route, environment, cancellation).await?),
        None => None,
    };
    let timeout = connect_timeout(&opts);
    let raw = connect_with_timeout(driver, opts, timeout).await?;
    Ok((raw, tunnel))
}

fn validate_client_tls_auth(driver: &dyn DatabaseDriver, opts: &ConnectOptions) -> Result<(), TransportError> {
    let (Some(cert), Some(key)) = (&opts.tls.client_cert, &opts.tls.client_key) else {
        if opts.tls.client_cert.is_none() && opts.tls.client_key.is_none() {
            return Ok(());
        }
        return Err(DriverError::Unsupported(
            "TLS client authentication requires both a certificate and a private key".into(),
        )
        .into());
    };
    if !driver.supports_client_tls_auth() {
        return Err(DriverError::Unsupported(format!(
            "{} does not support TLS client authentication",
            driver.display_name()
        ))
        .into());
    }
    if matches!(opts.tls.mode, TlsMode::Disabled | TlsMode::Prefer) {
        return Err(DriverError::Unsupported(
            "TLS client authentication requires a TLS mode that cannot fall back to plaintext".into(),
        )
        .into());
    }
    for path in [cert, key] {
        let file = std::fs::File::open(path)
            .map_err(|_| DriverError::Unsupported("TLS client certificate or key file cannot be opened".into()))?;
        let metadata = file
            .metadata()
            .map_err(|_| DriverError::Unsupported("TLS client certificate or key file cannot be inspected".into()))?;
        if !metadata.is_file() || metadata.len() > 1024 * 1024 {
            return Err(DriverError::Unsupported(
                "TLS client certificate and key must be regular files no larger than 1 MiB".into(),
            )
            .into());
        }
    }
    Ok(())
}

fn connect_timeout(opts: &ConnectOptions) -> Duration {
    opts.connect_timeout_secs
        .filter(|seconds| *seconds > 0)
        .map_or(DATABASE_CONNECT_TIMEOUT, |seconds| {
            Duration::from_secs(u64::from(seconds))
        })
}

async fn open_tunnel(
    driver: &dyn DatabaseDriver,
    opts: &mut ConnectOptions,
    route: SshRoute,
    environment: &SshEnvironment,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<Tunnel, TransportError> {
    let socket_name = forwarded_socket_name(driver, opts.tls.mode, opts.port);
    open_tunnel_with(opts, route, environment, socket_name, cancellation).await
}

async fn open_tunnel_with(
    opts: &mut ConnectOptions,
    route: SshRoute,
    environment: &SshEnvironment,
    socket_name: Option<String>,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<Tunnel, TransportError> {
    let remote = (std::mem::take(&mut opts.host), opts.port);
    let tunnel = match route {
        SshRoute::Builtin(hops) => Tunnel::Builtin(open_builtin(&hops, &remote, socket_name, environment).await?),
        SshRoute::OpenSsh(config) => Tunnel::OpenSsh(
            route::open_openssh(&config, environment, (&remote.0, remote.1), socket_name, cancellation).await?,
        ),
    };
    match (&tunnel, tunnel.socket_dir()) {
        (_, Some(directory)) => forward_through_socket(opts, remote, directory.to_path_buf()),
        (Tunnel::Builtin(tun), None) => {
            redirect_through_tunnel(opts, remote, (tun.local_host().to_string(), tun.local_port()))
        }
        (Tunnel::OpenSsh(forward), None) => match forward.local_endpoint() {
            tablepro_ssh::openssh::LocalEndpoint::Tcp(address) => {
                redirect_through_tunnel(opts, remote, (address.ip().to_string(), address.port()))
            }
            tablepro_ssh::openssh::LocalEndpoint::Unix(_) => {
                return Err(TransportError::Ssh("the forwarded socket has no directory".into()));
            }
        },
    }
    Ok(tunnel)
}

async fn open_builtin(
    hops: &[SshConfig],
    remote: &(String, u16),
    socket_name: Option<String>,
    environment: &SshEnvironment,
) -> Result<SshTunnel, TransportError> {
    if hops.is_empty() {
        return Err(TransportError::Ssh("jump chain is empty".into()));
    }
    let unknown = environment.unknown_host_key;
    match (&socket_name, &environment.builtin_prompter) {
        (Some(name), Some(prompter)) => {
            SshTunnel::open_chain_socket_prompted(hops, remote.0.clone(), remote.1, name, unknown, prompter.clone())
                .await
        }
        (None, Some(prompter)) => {
            SshTunnel::open_chain_prompted(hops, remote.0.clone(), remote.1, unknown, prompter.clone()).await
        }
        (Some(name), None) => SshTunnel::open_chain_socket(hops, remote.0.clone(), remote.1, name, unknown).await,
        (None, None) => SshTunnel::open_chain(hops, remote.0.clone(), remote.1, unknown).await,
    }
    .map_err(|error| match error {
        error @ tablepro_ssh::SshError::UnknownHostKey { .. } => TransportError::HostKeyRefused(error.to_string()),
        error @ tablepro_ssh::SshError::HostKeyMismatch { .. } => TransportError::HostKeyChanged(error.to_string()),
        error @ tablepro_ssh::SshError::Timeout { .. } => TransportError::SshTimeout(error.to_string()),
        error => TransportError::Ssh(error.to_string()),
    })
}

async fn connect_with_timeout(
    driver: &dyn DatabaseDriver,
    opts: ConnectOptions,
    timeout: Duration,
) -> Result<Box<dyn Connection>, TransportError> {
    tokio::time::timeout(timeout, driver.connect(opts))
        .await
        .map_err(|_| TransportError::DatabaseTimeout {
            seconds: timeout.as_secs(),
        })?
        .map_err(TransportError::Driver)
}

fn redirect_through_tunnel(opts: &mut ConnectOptions, service_endpoint: (String, u16), dial_endpoint: (String, u16)) {
    opts.service_endpoint = Some(service_endpoint);
    opts.host = dial_endpoint.0;
    opts.port = dial_endpoint.1;
    opts.forwarded_socket_dir = None;
}

fn forward_through_socket(opts: &mut ConnectOptions, service_endpoint: (String, u16), directory: std::path::PathBuf) {
    opts.host = service_endpoint.0.clone();
    opts.port = service_endpoint.1;
    opts.service_endpoint = Some(service_endpoint);
    opts.forwarded_socket_dir = Some(directory);
}

fn forwarded_socket_name(driver: &dyn DatabaseDriver, tls_mode: TlsMode, service_port: u16) -> Option<String> {
    if !tls_mode.verifies_cert() {
        return None;
    }
    driver.forwarded_socket_name(service_port)
}

fn check_auth_mode(mode: AuthMode, supports_integrated_auth: bool, driver_name: &str) -> Result<(), TransportError> {
    if mode == AuthMode::Kerberos && !supports_integrated_auth {
        return Err(TransportError::IntegratedAuthUnsupported(driver_name.to_string()));
    }
    Ok(())
}

fn jump_hop_password_refused(hop_index: usize) -> TransportError {
    TransportError::Secret(format!(
        "jump hop {hop_index} uses password auth, but only hop 0 can \
         (edit connections.json jump auth to use a private key for this hop)"
    ))
}

fn loads_database_password(saved: &SavedConnection) -> bool {
    saved.auth_mode == AuthMode::Password && !matches!(saved.driver_id.as_str(), "sqlite" | "duckdb")
}

async fn resolve_saved_ssh_chain(id: Uuid, saved: &SavedSshConfig) -> Result<Vec<SshConfig>, TransportError> {
    let hops = saved.flatten_hops();
    let mut out = Vec::with_capacity(hops.len());
    for (index, hop) in hops.into_iter().enumerate() {
        out.push(resolve_saved_ssh_hop(id, hop, index).await?);
    }
    Ok(out)
}

pub(crate) async fn resolve_saved_ssh_hop(
    id: Uuid,
    saved: &SavedSshConfig,
    hop_index: usize,
) -> Result<SshConfig, TransportError> {
    let auth = match &saved.auth {
        _ if saved.agent => match &saved.agent_identity {
            Some(public_key) => SshAuth::AgentIdentity {
                public_key: public_key.clone(),
            },
            None => SshAuth::Agent,
        },
        SavedSshAuth::Password if saved.credential_revision == 0 && hop_index > 0 => {
            return Err(jump_hop_password_refused(hop_index));
        }
        SavedSshAuth::Password => SshAuth::Password {
            password: saved_ssh_password(id, saved, hop_index).await?,
        },
        SavedSshAuth::PrivateKey { path, has_passphrase } => SshAuth::PrivateKey {
            path: path.clone(),
            passphrase: saved_ssh_passphrase(id, saved, hop_index, *has_passphrase).await?,
        },
    };
    Ok(SshConfig {
        host: saved.host.clone(),
        port: saved.port,
        username: saved.username.clone(),
        auth,
    })
}

pub(crate) fn secret_error(context: &str, error: tablepro_storage::StorageError) -> TransportError {
    match error {
        tablepro_storage::StorageError::Keyring(failure) => TransportError::Keyring(failure),
        other => TransportError::Secret(format!("{context}: {other}")),
    }
}

async fn saved_ssh_password(id: Uuid, hop: &SavedSshConfig, hop_index: usize) -> Result<SecretString, TransportError> {
    let password = if hop.credential_revision == 0 {
        load_ssh_password(id)
            .await
            .map_err(|e| secret_error("load ssh password", e))?
    } else {
        tablepro_storage::load_ssh_hop_password(id, hop.hop_id, hop.credential_revision)
            .await
            .map_err(|e| secret_error("load ssh hop password", e))?
    };
    password.ok_or_else(|| TransportError::Secret(format!("ssh password for hop {hop_index} not in keyring")))
}

async fn saved_ssh_passphrase(
    id: Uuid,
    hop: &SavedSshConfig,
    hop_index: usize,
    has_passphrase: bool,
) -> Result<Option<SecretString>, TransportError> {
    if !has_passphrase {
        return Ok(None);
    }
    if hop.credential_revision == 0 {
        if hop_index > 0 {
            return Err(TransportError::Secret(format!(
                "legacy SSH passphrase cannot be assigned to jump hop {hop_index}"
            )));
        }
        return load_ssh_passphrase(id)
            .await
            .map_err(|e| secret_error("load ssh passphrase", e));
    }
    tablepro_storage::load_ssh_hop_passphrase(id, hop.hop_id, hop.credential_revision)
        .await
        .map_err(|e| secret_error("load ssh hop passphrase", e))
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
mod credential_tests;
