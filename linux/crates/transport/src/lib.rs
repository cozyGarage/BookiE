use secrecy::SecretString;
use std::os::unix::fs::FileTypeExt;
use std::time::Duration;
use tablepro_core::{AuthMode, ConnectOptions, Connection, DatabaseDriver, DriverError, TlsConfig, TlsMode};
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

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("ssh: {0}")]
    Ssh(String),
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
            ..Default::default()
        },
        auth_mode: saved.auth_mode,
        service_endpoint: None,
        local_socket_dir: saved.socket_dir.clone(),
        forwarded_socket_dir: None,
        application_name: None,
        connect_timeout_secs: saved.connect_timeout_secs,
    })
}

pub async fn saved_ssh_route(saved: &SavedConnection) -> Result<Option<SshRoute>, TransportError> {
    let Some(ssh) = &saved.ssh else {
        return Ok(None);
    };
    match ssh.client {
        SshClient::Builtin => resolve_saved_ssh_chain(saved.id, ssh)
            .await
            .map(|hops| Some(SshRoute::Builtin(hops))),
        SshClient::OpenSsh => route::openssh_config(saved.id, ssh)
            .await
            .map(|config| Some(SshRoute::OpenSsh(config))),
    }
}

pub async fn establish(
    driver: &dyn DatabaseDriver,
    mut opts: ConnectOptions,
    ssh: Option<SshRoute>,
    environment: &SshEnvironment,
) -> Result<(Box<dyn Connection>, Option<Tunnel>), TransportError> {
    check_auth_mode(opts.auth_mode, driver.supports_integrated_auth(), driver.display_name())?;
    validate_local_socket(&opts, driver, ssh.is_some())?;
    opts.forwarded_socket_dir = None;
    let tunnel = match ssh {
        Some(route) => Some(open_tunnel(driver, &mut opts, route, environment).await?),
        None => None,
    };
    let timeout = connect_timeout(&opts);
    let raw = connect_with_timeout(driver, opts, timeout).await?;
    Ok((raw, tunnel))
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
) -> Result<Tunnel, TransportError> {
    let remote = (std::mem::take(&mut opts.host), opts.port);
    let socket_name = forwarded_socket_name(driver, opts.tls.mode, remote.1);
    let tunnel = match route {
        SshRoute::Builtin(hops) => Tunnel::Builtin(open_builtin(&hops, &remote, socket_name, environment).await?),
        SshRoute::OpenSsh(config) => {
            Tunnel::OpenSsh(route::open_openssh(&config, environment, (&remote.0, remote.1), socket_name).await?)
        }
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
    match &socket_name {
        Some(name) => SshTunnel::open_chain_socket(hops, remote.0.clone(), remote.1, name, unknown).await,
        None => SshTunnel::open_chain(hops, remote.0.clone(), remote.1, unknown).await,
    }
    .map_err(|e| TransportError::Ssh(e.to_string()))
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

fn validate_local_socket(
    opts: &ConnectOptions,
    driver: &dyn DatabaseDriver,
    has_ssh: bool,
) -> Result<(), TransportError> {
    let Some(directory) = opts.local_socket_dir.as_deref() else {
        return Ok(());
    };
    if !driver.supports_local_socket() {
        return Err(TransportError::LocalSocketUnsupported(
            driver.display_name().to_string(),
        ));
    }
    if has_ssh {
        return Err(TransportError::LocalSocketWithSsh);
    }
    if opts.tls.mode != TlsMode::Disabled {
        return Err(TransportError::LocalSocketWithTls);
    }
    if opts.port == 0 {
        return Err(TransportError::InvalidLocalSocket(
            "PostgreSQL socket port must be greater than zero".into(),
        ));
    }
    if !directory.is_absolute() {
        return Err(TransportError::InvalidLocalSocket(format!(
            "socket directory must be absolute: {}",
            directory.display()
        )));
    }
    let metadata = std::fs::metadata(directory).map_err(|error| {
        TransportError::InvalidLocalSocket(format!(
            "cannot access socket directory {}: {error}",
            directory.display()
        ))
    })?;
    if !metadata.is_dir() {
        return Err(TransportError::InvalidLocalSocket(format!(
            "socket path is not a directory: {}",
            directory.display()
        )));
    }
    let socket = directory.join(format!(".s.PGSQL.{}", opts.port));
    match std::fs::metadata(&socket) {
        Ok(metadata) if metadata.file_type().is_socket() => {}
        Ok(_) => {
            return Err(TransportError::InvalidLocalSocket(format!(
                "PostgreSQL socket path is not a socket: {}",
                socket.display()
            )));
        }
        Err(error) => {
            return Err(TransportError::InvalidLocalSocket(format!(
                "cannot access PostgreSQL socket {}: {error}",
                socket.display()
            )));
        }
    }
    Ok(())
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
        SavedSshAuth::Password if hop_index > 0 => {
            // The keyring holds one SSH password per connection, not per hop
            // (storage::secrets keys on connection id and secret kind only),
            // so a jump hop cannot have its own password. Reusing hop 0's
            // password here would send it to a different host silently; fail
            // closed instead.
            return Err(jump_hop_password_refused(hop_index));
        }
        SavedSshAuth::Password => SshAuth::Password {
            password: saved_ssh_password(id).await?,
        },
        SavedSshAuth::PrivateKey { path, has_passphrase } => SshAuth::PrivateKey {
            path: path.clone(),
            passphrase: saved_ssh_passphrase(id, *has_passphrase).await?,
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

async fn saved_ssh_password(id: Uuid) -> Result<SecretString, TransportError> {
    load_ssh_password(id)
        .await
        .map_err(|e| secret_error("load ssh password", e))?
        .ok_or_else(|| TransportError::Secret("ssh password not in keyring".into()))
}

async fn saved_ssh_passphrase(id: Uuid, has_passphrase: bool) -> Result<Option<SecretString>, TransportError> {
    if !has_passphrase {
        return Ok(None);
    }
    load_ssh_passphrase(id)
        .await
        .map_err(|e| secret_error("load ssh passphrase", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_connect_timeout_replaces_the_default_and_zero_keeps_it() {
        let mut opts = ConnectOptions::default();
        assert_eq!(connect_timeout(&opts), DATABASE_CONNECT_TIMEOUT);
        opts.connect_timeout_secs = Some(0);
        assert_eq!(connect_timeout(&opts), DATABASE_CONNECT_TIMEOUT);
        opts.connect_timeout_secs = Some(5);
        assert_eq!(connect_timeout(&opts), Duration::from_secs(5));
    }

    struct HangingDriver;

    #[async_trait::async_trait]
    impl DatabaseDriver for HangingDriver {
        fn id(&self) -> &'static str {
            "hanging"
        }

        fn display_name(&self) -> &'static str {
            "Hanging"
        }

        fn default_port(&self) -> u16 {
            1
        }

        async fn connect(&self, _opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
            std::future::pending().await
        }
    }

    #[test]
    fn tunnel_uses_local_dial_endpoint_and_keeps_service_identity() {
        let mut opts = ConnectOptions {
            host: "sql.corp.example".into(),
            port: 1433,
            ..Default::default()
        };

        redirect_through_tunnel(
            &mut opts,
            ("sql.corp.example".into(), 1433),
            ("127.0.0.1".into(), 54321),
        );

        assert_eq!(opts.host, "127.0.0.1");
        assert_eq!(opts.port, 54321);
        assert_eq!(opts.service_address(), ("sql.corp.example", 1433));
    }

    struct SocketDriver;

    #[async_trait::async_trait]
    impl DatabaseDriver for SocketDriver {
        fn id(&self) -> &'static str {
            "socket"
        }
        fn display_name(&self) -> &'static str {
            "Socket"
        }
        fn default_port(&self) -> u16 {
            5432
        }
        fn forwarded_socket_name(&self, service_port: u16) -> Option<String> {
            Some(format!(".s.PGSQL.{service_port}"))
        }
        fn supports_local_socket(&self) -> bool {
            true
        }
        async fn connect(&self, _opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
            Err(DriverError::Unsupported("test driver".into()))
        }
    }

    struct TcpOnlyDriver;

    #[async_trait::async_trait]
    impl DatabaseDriver for TcpOnlyDriver {
        fn id(&self) -> &'static str {
            "tcp-only"
        }
        fn display_name(&self) -> &'static str {
            "TCP only"
        }
        fn default_port(&self) -> u16 {
            3306
        }
        async fn connect(&self, _opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
            Err(DriverError::Unsupported("test driver".into()))
        }
    }

    #[test]
    fn socket_forwarding_applies_only_when_the_mode_verifies_certificates() {
        assert_eq!(
            forwarded_socket_name(&SocketDriver, TlsMode::VerifyFull, 5432).as_deref(),
            Some(".s.PGSQL.5432")
        );
        assert_eq!(
            forwarded_socket_name(&SocketDriver, TlsMode::VerifyCa, 5432).as_deref(),
            Some(".s.PGSQL.5432")
        );
        assert!(forwarded_socket_name(&SocketDriver, TlsMode::Require, 5432).is_none());
        assert!(forwarded_socket_name(&SocketDriver, TlsMode::Disabled, 5432).is_none());
        assert!(forwarded_socket_name(&TcpOnlyDriver, TlsMode::VerifyFull, 3306).is_none());
    }

    #[test]
    fn local_socket_validation_is_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let mut opts = ConnectOptions {
            host: "localhost".into(),
            port: 5432,
            local_socket_dir: Some(dir.path().to_path_buf()),
            ..Default::default()
        };

        assert!(matches!(
            validate_local_socket(&opts, &TcpOnlyDriver, false),
            Err(TransportError::LocalSocketUnsupported(_))
        ));
        assert!(matches!(
            validate_local_socket(&opts, &SocketDriver, true),
            Err(TransportError::LocalSocketWithSsh)
        ));

        opts.tls = tls_config(TlsMode::VerifyFull);
        assert!(matches!(
            validate_local_socket(&opts, &SocketDriver, false),
            Err(TransportError::LocalSocketWithTls)
        ));
        opts.tls = tls_config(TlsMode::Disabled);
        assert!(matches!(
            validate_local_socket(&opts, &SocketDriver, false),
            Err(TransportError::InvalidLocalSocket(_))
        ));
        opts.port = 0;
        assert!(matches!(
            validate_local_socket(&opts, &SocketDriver, false),
            Err(TransportError::InvalidLocalSocket(_))
        ));
        opts.port = 5432;
        opts.local_socket_dir = Some(std::path::PathBuf::from("relative"));
        assert!(matches!(
            validate_local_socket(&opts, &SocketDriver, false),
            Err(TransportError::InvalidLocalSocket(_))
        ));
    }

    #[test]
    fn socket_forwarding_keeps_the_service_hostname_for_tls() {
        let mut opts = ConnectOptions {
            host: String::new(),
            port: 5432,
            tls: tls_config(TlsMode::VerifyFull),
            ..Default::default()
        };

        forward_through_socket(
            &mut opts,
            ("db.corp.example".into(), 5432),
            std::path::PathBuf::from("/run/user/1000/tablepro-ssh-abc"),
        );

        assert_eq!(opts.service_address(), ("db.corp.example", 5432));
        assert_eq!(
            opts.transport(),
            tablepro_core::Transport::Socket {
                directory: std::path::Path::new("/run/user/1000/tablepro-ssh-abc"),
                identity_host: "db.corp.example",
                identity_port: 5432,
                origin: tablepro_core::SocketOrigin::Forwarded,
            }
        );
    }

    #[test]
    fn tcp_forwarding_clears_any_earlier_socket_directory() {
        let mut opts = ConnectOptions {
            host: "db.corp.example".into(),
            port: 5432,
            forwarded_socket_dir: Some(std::path::PathBuf::from("/run/user/1000/stale")),
            ..Default::default()
        };

        redirect_through_tunnel(&mut opts, ("db.corp.example".into(), 5432), ("127.0.0.1".into(), 54321));

        assert!(opts.forwarded_socket_dir.is_none());
        assert_eq!(
            opts.transport(),
            tablepro_core::Transport::Tcp {
                host: "127.0.0.1",
                port: 54321
            }
        );
    }

    #[tokio::test]
    async fn establish_never_reuses_a_socket_directory_from_an_earlier_attempt() {
        let opts = ConnectOptions {
            host: "db.corp.example".into(),
            port: 5432,
            forwarded_socket_dir: Some(std::path::PathBuf::from("/run/user/1000/removed")),
            ..Default::default()
        };

        let error = establish(
            &SocketDriver,
            opts,
            None,
            &SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Learn),
        )
        .await
        .err()
        .expect("test driver refuses to connect");

        assert!(error.to_string().contains("test driver"), "unexpected error: {error}");
    }

    #[tokio::test]
    async fn database_connection_attempt_is_bounded() {
        let error = connect_with_timeout(&HangingDriver, ConnectOptions::default(), Duration::from_millis(1))
            .await
            .err()
            .expect("hanging connection must time out");

        assert!(matches!(error, TransportError::DatabaseTimeout { .. }));
    }

    #[test]
    fn kerberos_requires_driver_support() {
        assert!(check_auth_mode(AuthMode::Kerberos, false, "PostgreSQL").is_err());
        assert!(check_auth_mode(AuthMode::Kerberos, true, "SQL Server").is_ok());
        assert!(check_auth_mode(AuthMode::Password, false, "PostgreSQL").is_ok());
    }

    fn key_hop(host: &str) -> SavedSshConfig {
        SavedSshConfig {
            host: host.into(),
            port: 22,
            username: "svc".into(),
            auth: SavedSshAuth::PrivateKey {
                path: "/home/svc/.ssh/id_ed25519".into(),
                has_passphrase: false,
            },
            jump: None,
            client: Default::default(),
        }
    }

    #[tokio::test]
    async fn a_jump_hop_cannot_use_password_auth() {
        let bastion = SavedSshConfig {
            jump: Some(Box::new(SavedSshConfig {
                auth: SavedSshAuth::Password,
                ..key_hop("jump2.partner")
            })),
            ..key_hop("bastion.corp")
        };

        let error = resolve_saved_ssh_chain(Uuid::new_v4(), &bastion)
            .await
            .expect_err("a jump hop with password auth must be refused");

        let message = error.to_string();
        assert!(message.contains("hop 1"), "unexpected error: {message}");
        assert!(
            !message.contains("not in keyring"),
            "must fail before ever touching the keyring: {message}"
        );
    }

    #[tokio::test]
    async fn a_multi_hop_key_only_chain_still_resolves() {
        let bastion = SavedSshConfig {
            jump: Some(Box::new(key_hop("jump2.partner"))),
            ..key_hop("bastion.corp")
        };

        let chain = resolve_saved_ssh_chain(Uuid::new_v4(), &bastion)
            .await
            .expect("a chain with only key-based hops must resolve");

        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].host, "bastion.corp");
        assert_eq!(chain[1].host, "jump2.partner");
    }

    #[tokio::test]
    #[ignore = "requires a running Secret Service; scripts/test-secret-service.sh provides one"]
    async fn hop_zero_password_auth_still_works() {
        let bastion = SavedSshConfig {
            auth: SavedSshAuth::Password,
            jump: Some(Box::new(key_hop("jump2.partner"))),
            ..key_hop("bastion.corp")
        };

        let error = resolve_saved_ssh_chain(Uuid::new_v4(), &bastion).await.unwrap_err();
        assert!(
            error.to_string().contains("not in keyring"),
            "hop 0 password auth must still reach the keyring lookup: {error}"
        );
    }
}
