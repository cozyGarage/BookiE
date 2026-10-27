use super::*;
use crate::credential_tests::key_hop;
use tablepro_policy::{AuditEvent, AuditTerminalStatus, AuditTransportClient};

#[path = "tests/../tool_endpoint_tests.rs"]
mod tool_endpoint_tests;

#[derive(Default)]
struct RecordingAudit(std::sync::Mutex<Vec<AuditEvent>>);

#[async_trait::async_trait]
impl AuditSink for RecordingAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), tablepro_policy::AuditError> {
        self.0.lock().expect("audit events").push(event);
        Ok(())
    }
}

struct FailingAudit;

#[async_trait::async_trait]
impl AuditSink for FailingAudit {
    async fn record(&self, _event: AuditEvent) -> Result<(), tablepro_policy::AuditError> {
        Err(tablepro_policy::AuditError::Persistence("disk full".into()))
    }
}

fn audit_environment(sink: Arc<dyn AuditSink>, state: Arc<AuditState>) -> SshEnvironment {
    SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Refuse).with_audit(TransportAuditContext::new(
        sink,
        state,
        Principal::Agent {
            token: "secret-agent-token".into(),
            client: None,
            model: None,
        },
        Uuid::new_v4(),
        "test connection",
        Environment::Local,
        "postgres",
    ))
}

fn saved_ssh_connection(client: SshClient) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: "sandbox routing test".into(),
        driver_id: "postgres".into(),
        host: "db.example".into(),
        port: 5432,
        socket_dir: None,
        socket_path: None,
        database: "db".into(),
        username: "user".into(),
        use_tls: false,
        tls_mode: None,
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        auth_mode: AuthMode::Password,
        read_only: false,
        environment: Environment::Local,
        ssh: Some(SavedSshConfig {
            hop_id: Uuid::new_v4(),
            credential_revision: 0,
            host: "bastion.example".into(),
            port: 22,
            username: "user".into(),
            auth: SavedSshAuth::Password,
            jump: None,
            client,
            agent: false,
        }),
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
    }
}

#[tokio::test]
async fn saved_system_openssh_refuses_deterministically_in_flatpak_without_fallback() {
    let saved = saved_ssh_connection(SshClient::OpenSsh);
    assert!(matches!(
        saved_ssh_route_with_sandbox_override(&saved, Some(true)).await,
        Err(TransportError::SystemSshUnavailableInSandbox)
    ));

    let mut builtin = saved.clone();
    let builtin_ssh = builtin.ssh.as_mut().expect("ssh config");
    builtin_ssh.client = SshClient::Builtin;
    builtin_ssh.agent = true;
    assert!(matches!(
        saved_ssh_route_with_sandbox_override(&builtin, Some(true)).await,
        Ok(Some(SshRoute::Builtin(_)))
    ));

    let mut native = builtin;
    native.ssh.as_mut().expect("ssh config").client = SshClient::OpenSsh;
    assert!(matches!(
        saved_ssh_route_with_sandbox_override(&native, Some(false)).await,
        Ok(Some(SshRoute::OpenSsh(_)))
    ));
}

#[tokio::test]
async fn cancelled_ssh_attempt_writes_one_terminal_redacted_record() {
    let sink = Arc::new(RecordingAudit::default());
    let state = Arc::new(AuditState::new());
    let environment = audit_environment(sink.clone(), state);
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let result = establish_with_cancellation(
        &HangingDriver,
        ConnectOptions::default(),
        Some(SshRoute::Builtin(Vec::new())),
        &environment,
        cancellation,
    )
    .await;
    assert!(matches!(result, Err(TransportError::Cancelled)));
    let events = sink.0.lock().expect("audit events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].phase, tablepro_policy::AuditRecordPhase::Outcome);
    assert_eq!(events[0].terminal_status, AuditTerminalStatus::Cancelled);
    assert_eq!(
        events[0].transport_attempt.unwrap().client,
        AuditTransportClient::BuiltinSsh
    );
    match &events[0].principal {
        Principal::Agent { token, .. } => assert!(!token.contains("secret-agent-token")),
        Principal::Human { .. } => panic!("agent principal expected"),
    }
}

#[tokio::test]
async fn audit_persistence_failure_fails_closed_and_returns_no_connection() {
    let state = Arc::new(AuditState::new());
    let environment = audit_environment(Arc::new(FailingAudit), state.clone());
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let result = establish_with_cancellation(
        &HangingDriver,
        ConnectOptions::default(),
        Some(SshRoute::Builtin(Vec::new())),
        &environment,
        cancellation,
    )
    .await;
    assert!(matches!(result, Err(TransportError::AuditWriteFailed(_))));
    assert!(state.governed_writes_disabled());
}

#[tokio::test]
async fn a_tool_endpoint_without_ssh_is_the_saved_endpoint_and_is_not_audited() {
    let sink = Arc::new(RecordingAudit::default());
    let environment = audit_environment(sink.clone(), Arc::new(AuditState::new()));
    let mut opts = ConnectOptions {
        host: "db.example".into(),
        port: 5432,
        ..Default::default()
    };
    opts.forwarded_socket_dir = Some("/run/stale".into());
    let (endpoint, tunnel) = open_tool_endpoint(
        &HangingDriver,
        opts,
        None,
        &environment,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("endpoint");
    assert!(tunnel.is_none());
    assert_eq!((endpoint.host.as_str(), endpoint.port), ("db.example", 5432));
    assert!(endpoint.forwarded_socket_dir.is_none());
    assert!(sink.0.lock().expect("audit events").is_empty());
}

#[tokio::test]
async fn a_tool_endpoint_records_its_ssh_attempt_like_a_connection() {
    let sink = Arc::new(RecordingAudit::default());
    let environment = audit_environment(sink.clone(), Arc::new(AuditState::new()));
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let result = open_tool_endpoint(
        &HangingDriver,
        ConnectOptions::default(),
        Some(SshRoute::Builtin(Vec::new())),
        &environment,
        cancellation,
    )
    .await;
    assert!(matches!(result, Err(TransportError::Cancelled)));
    let failed = open_tool_endpoint(
        &HangingDriver,
        ConnectOptions::default(),
        Some(SshRoute::Builtin(Vec::new())),
        &environment,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert!(matches!(failed, Err(TransportError::Ssh(_))));
    let events = sink.0.lock().expect("audit events");
    let statuses: Vec<_> = events.iter().map(|event| event.terminal_status).collect();
    assert_eq!(statuses, [AuditTerminalStatus::Cancelled, AuditTerminalStatus::Failed]);
}

#[tokio::test]
async fn a_tool_endpoint_fails_closed_when_its_audit_cannot_be_written() {
    let state = Arc::new(AuditState::new());
    let environment = audit_environment(Arc::new(FailingAudit), state.clone());
    let result = open_tool_endpoint(
        &HangingDriver,
        ConnectOptions::default(),
        Some(SshRoute::Builtin(Vec::new())),
        &environment,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, Err(TransportError::AuditWriteFailed(_))));
    assert!(state.governed_writes_disabled());
}

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
    fn supports_local_socket_file(&self) -> bool {
        true
    }
    fn supports_local_socket_directory(&self) -> bool {
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
fn local_socket_file_validation_requires_an_absolute_unix_socket_without_tls_or_ssh() {
    use std::os::unix::net::UnixListener;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mysqld.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let mut opts = ConnectOptions {
        host: "localhost".into(),
        port: 3306,
        local_socket_path: Some(path.clone()),
        ..Default::default()
    };

    assert!(validate_local_socket(&opts, &SocketDriver, false).is_ok());
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
    opts.local_socket_path = Some(std::path::PathBuf::from("relative.sock"));
    assert!(matches!(
        validate_local_socket(&opts, &SocketDriver, false),
        Err(TransportError::InvalidLocalSocket(_))
    ));
    opts.local_socket_path = Some(directory.path().join("missing.sock"));
    assert!(matches!(
        validate_local_socket(&opts, &SocketDriver, false),
        Err(TransportError::InvalidLocalSocket(_))
    ));
    opts.local_socket_path = Some(directory.path().join("regular-file"));
    std::fs::write(opts.local_socket_path.as_ref().expect("socket path"), b"not a socket").unwrap();
    assert!(matches!(
        validate_local_socket(&opts, &SocketDriver, false),
        Err(TransportError::InvalidLocalSocket(_))
    ));
    drop(listener);
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

#[tokio::test]
#[ignore = "requires a running Secret Service; scripts/test-secret-service.sh provides one"]
async fn password_auth_uses_the_secret_bound_to_each_hop() {
    let connection_id = Uuid::new_v4();
    let root_secret = Uuid::new_v4().to_string();
    let jump_secret = Uuid::new_v4().to_string();
    let root = SavedSshConfig {
        hop_id: Uuid::new_v4(),
        credential_revision: 1,
        auth: SavedSshAuth::Password,
        jump: Some(Box::new(SavedSshConfig {
            hop_id: Uuid::new_v4(),
            credential_revision: 1,
            auth: SavedSshAuth::Password,
            ..key_hop("jump.example")
        })),
        ..key_hop("root.example")
    };
    let hops = root.flatten_hops();
    tablepro_storage::store_ssh_hop_password(
        connection_id,
        hops[0].hop_id,
        hops[0].credential_revision,
        &root_secret,
        "BookiE test root SSH password",
    )
    .await
    .unwrap();
    tablepro_storage::store_ssh_hop_password(
        connection_id,
        hops[1].hop_id,
        hops[1].credential_revision,
        &jump_secret,
        "BookiE test jump SSH password",
    )
    .await
    .unwrap();

    let resolved = resolve_saved_ssh_chain(connection_id, &root).await.unwrap();
    let secrets = resolved
        .iter()
        .map(|hop| match &hop.auth {
            SshAuth::Password { password } => secrecy::ExposeSecret::expose_secret(password).to_owned(),
            _ => panic!("expected password auth"),
        })
        .collect::<Vec<_>>();

    for hop in hops {
        tablepro_storage::delete_ssh_hop_password(connection_id, hop.hop_id, hop.credential_revision)
            .await
            .unwrap();
    }
    assert_eq!(secrets, [root_secret, jump_secret]);
}

#[path = "tests/ssh_passphrase_tests.rs"]
mod ssh_passphrase_tests;

#[tokio::test]
#[ignore = "requires a running Secret Service; scripts/test-secret-service.sh provides one"]
async fn legacy_jump_key_passphrase_is_refused_before_keyring_access() {
    let connection_id = Uuid::new_v4();
    tablepro_storage::store_ssh_passphrase(connection_id, "root-passphrase", "BookiE test legacy SSH passphrase")
        .await
        .unwrap();
    let bastion = SavedSshConfig {
        jump: Some(Box::new(SavedSshConfig {
            auth: SavedSshAuth::PrivateKey {
                path: "/unused/jump-key".into(),
                has_passphrase: true,
            },
            ..key_hop("jump.example")
        })),
        ..key_hop("root.example")
    };

    let error = resolve_saved_ssh_chain(connection_id, &bastion)
        .await
        .expect_err("legacy root passphrase must not be reused for a jump key");
    tablepro_storage::delete_ssh_passphrase(connection_id).await.unwrap();

    let message = error.to_string();
    assert!(message.contains("hop 1"), "unexpected error: {message}");
    assert!(
        !message.contains("keyring"),
        "must refuse before keyring access: {message}"
    );
}
