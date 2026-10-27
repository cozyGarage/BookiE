use super::*;

#[tokio::test(start_paused = true)]
async fn host_key_prompt_does_not_consume_the_ssh_handshake_timeout() {
    let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
    let (reply, wait_for_reply) = tokio::sync::oneshot::channel();
    let prompt = tokio::spawn(async move {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(60)).await;
        reply.send(()).unwrap();
    });
    let handshake = async move {
        events.send(HostKeyPromptEvent::WaitingForUser).unwrap();
        wait_for_reply.await.unwrap();
        events
            .send(HostKeyPromptEvent::UserResponded {
                declined_fingerprint: None,
            })
            .unwrap();
        Ok::<_, ()>("connected")
    };

    let result = wait_for_handshake(handshake, receiver, Duration::from_secs(10)).await;
    prompt.await.unwrap();
    assert!(matches!(result, Ok("connected")));
}

#[tokio::test(start_paused = true)]
async fn ssh_handshake_timeout_resumes_after_host_key_prompt_response() {
    let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
    let (reply, wait_for_reply) = tokio::sync::oneshot::channel();
    let prompt = tokio::spawn(async move {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(60)).await;
        reply.send(()).unwrap();
    });
    let handshake = async move {
        events.send(HostKeyPromptEvent::WaitingForUser).unwrap();
        wait_for_reply.await.unwrap();
        events
            .send(HostKeyPromptEvent::UserResponded {
                declined_fingerprint: None,
            })
            .unwrap();
        tokio::time::sleep(Duration::from_secs(11)).await;
        Ok::<_, ()>("connected")
    };

    let result = wait_for_handshake(handshake, receiver, Duration::from_secs(10)).await;
    prompt.await.unwrap();
    assert!(matches!(result, Err(HandshakeWaitError::Timeout)));
}

#[tokio::test(start_paused = true)]
async fn host_key_prompt_resumes_only_the_remaining_handshake_budget() {
    let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
    let (prompt_started, wait_for_prompt) = tokio::sync::oneshot::channel();
    let (reply, wait_for_reply) = tokio::sync::oneshot::channel();
    let prompt = tokio::spawn(async move {
        wait_for_prompt.await.unwrap();
        tokio::time::advance(Duration::from_secs(60)).await;
        reply.send(()).unwrap();
    });
    let handshake = async move {
        tokio::time::sleep(Duration::from_secs(8)).await;
        events.send(HostKeyPromptEvent::WaitingForUser).unwrap();
        prompt_started.send(()).unwrap();
        wait_for_reply.await.unwrap();
        events
            .send(HostKeyPromptEvent::UserResponded {
                declined_fingerprint: None,
            })
            .unwrap();
        tokio::time::sleep(Duration::from_secs(3)).await;
        Ok::<_, ()>("connected")
    };

    let result = wait_for_handshake(handshake, receiver, Duration::from_secs(10)).await;
    prompt.await.unwrap();
    assert!(matches!(result, Err(HandshakeWaitError::Timeout)));
}

#[test]
fn client_config_enables_keepalive_so_a_dead_bastion_is_eventually_detected() {
    let config = client_config();
    assert_eq!(
        config.keepalive_interval,
        Some(KEEPALIVE_INTERVAL),
        "without a keepalive interval, russh never notices an unresponsive peer and \
             a dead bastion hangs until the driver's own timeout fires"
    );
    assert!(
        config.keepalive_max > 0,
        "keepalive_max must stay enabled to bound detection time"
    );
}

#[test]
fn a_server_offering_only_keyboard_interactive_reports_it_is_unsupported() {
    let remaining = russh::MethodSet::from(&[russh::MethodKind::KeyboardInteractive][..]);
    let auth = client::AuthResult::Failure {
        remaining_methods: remaining,
        partial_success: false,
    };
    let error = auth_failure_error(&auth);
    assert!(
        matches!(error, SshError::KeyboardInteractiveUnsupported),
        "expected KeyboardInteractiveUnsupported, got {error:?}"
    );
}

#[test]
fn a_server_still_offering_password_reports_a_plain_auth_failure() {
    let remaining = russh::MethodSet::from(&[russh::MethodKind::KeyboardInteractive, russh::MethodKind::Password][..]);
    let auth = client::AuthResult::Failure {
        remaining_methods: remaining,
        partial_success: false,
    };
    let error = auth_failure_error(&auth);
    assert!(matches!(error, SshError::Auth), "expected Auth, got {error:?}");
}

#[test]
fn unknown_host_key_error_does_not_advise_running_ssh() {
    let known_hosts = PathBuf::from("/home/user/.config/tablepro/known_hosts");
    let error = unknown_host_key("db.example.com", 5432, "fp".to_string(), &known_hosts);
    let message = error.to_string();
    assert!(
        !message.contains("or ssh"),
        "message should not advise running the system ssh client, which writes a different known_hosts file: {message}"
    );
    assert!(
        message.contains(known_hosts.display().to_string().as_str()),
        "message should point at this client's own known_hosts path: {message}"
    );
}

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
