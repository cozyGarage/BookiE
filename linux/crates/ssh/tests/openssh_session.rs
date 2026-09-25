#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;
use tablepro_ssh::openssh::{
    AskpassPrompt, ForwardRoute, ForwardTarget, LocalEndpoint, OpenSshAuth, OpenSshConfig, OpenSshContext,
    OpenSshError, OpenSshRuntime, OpenSshSession, OpenSshTimeouts, PromptAnswer, Prompter, SshDestination,
};
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

type TestResult<T> = Result<T, Box<dyn Error>>;

const SSHD_SCRIPT: &str = "apk add --no-cache openssh >/dev/null \
    && ssh-keygen -A >/dev/null \
    && adduser -D deploy && echo 'deploy:s3cret' | chpasswd \
    && adduser -D ops && echo 'ops:s3cret' | chpasswd \
    && exec /usr/sbin/sshd -D -e -p 2222 -o PasswordAuthentication=yes -o KbdInteractiveAuthentication=no -o AllowTcpForwarding=yes";

struct Fixture {
    _container: ContainerAsync<GenericImage>,
    _temp: tempfile::TempDir,
    host: String,
    port: u16,
    known_hosts: PathBuf,
    context: OpenSshContext,
}

#[derive(Default)]
struct ScriptedPrompter {
    answers: Mutex<Vec<PromptAnswer>>,
    asked: Mutex<Vec<AskpassPrompt>>,
}

impl ScriptedPrompter {
    fn new(mut answers: Vec<PromptAnswer>) -> Arc<Self> {
        answers.reverse();
        Arc::new(Self {
            answers: Mutex::new(answers),
            asked: Mutex::new(Vec::new()),
        })
    }

    fn asked(&self) -> Vec<AskpassPrompt> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait]
impl Prompter for ScriptedPrompter {
    async fn answer(&self, prompt: &AskpassPrompt) -> PromptAnswer {
        self.asked.lock().unwrap().push(prompt.clone());
        self.answers.lock().unwrap().pop().unwrap_or(PromptAnswer::Decline)
    }
}

async fn start() -> TestResult<Fixture> {
    let container = GenericImage::new("alpine", "3.22")
        .with_exposed_port(2222.tcp())
        .with_wait_for(WaitFor::message_on_stderr("Server listening on"))
        .with_cmd(["sh", "-c", SSHD_SCRIPT])
        .start()
        .await?;
    let host = container.get_host().await?.to_string();
    let port = container.get_host_port_ipv4(2222).await?;

    let temp = tempfile::tempdir()?;
    let known_hosts = temp.path().join("known_hosts");
    let config = temp.path().join("ssh_config");
    std::fs::write(
        &config,
        format!(
            "Host *\n  UserKnownHostsFile {}\n  GlobalKnownHostsFile /dev/null\n  IdentityAgent none\n  IdentitiesOnly yes\n  PubkeyAuthentication no\n",
            known_hosts.display()
        ),
    )?;
    let wrapper = temp.path().join("ssh");
    std::fs::write(
        &wrapper,
        format!("#!/bin/sh\nexec ssh -F '{}' \"$@\"\n", config.display()),
    )?;
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))?;

    let base = temp.path().join("run");
    std::fs::create_dir(&base)?;
    let context = OpenSshContext {
        runtime: OpenSshRuntime::acquire(&base)?,
        ssh_program: wrapper,
        askpass_program: PathBuf::from(env!("CARGO_BIN_EXE_tablepro-askpass")),
        timeouts: OpenSshTimeouts::default(),
    };
    Ok(Fixture {
        _container: container,
        _temp: temp,
        host,
        port,
        known_hosts,
        context,
    })
}

fn config(fixture: &Fixture, auth: OpenSshAuth) -> OpenSshConfig {
    OpenSshConfig {
        destination: SshDestination::new(&fixture.host, Some(fixture.port), Some("deploy".to_owned())).unwrap(),
        jump_hosts: Vec::new(),
        auth,
    }
}

fn password(secret: &str) -> OpenSshAuth {
    OpenSshAuth::Password {
        password: SecretString::from(secret),
    }
}

async fn connect(
    fixture: &Fixture,
    config: &OpenSshConfig,
    answers: Vec<PromptAnswer>,
) -> (Result<Arc<OpenSshSession>, OpenSshError>, Arc<ScriptedPrompter>) {
    let prompter = ScriptedPrompter::new(answers);
    let result = OpenSshSession::connect(config, &fixture.context, prompter.clone(), CancellationToken::new()).await;
    (result, prompter)
}

fn process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[tokio::test]
#[ignore = "requires docker"]
async fn openssh_password_through_prompter() {
    let fixture = start().await.unwrap();
    let answers = vec![PromptAnswer::Accept, PromptAnswer::Secret(SecretString::from("s3cret"))];
    let (session, prompter) = connect(&fixture, &config(&fixture, OpenSshAuth::Agent), answers).await;
    let session = session.unwrap();

    assert!(session.master_pid().is_some_and(process_alive));
    let asked = prompter.asked();
    assert!(matches!(asked[0], AskpassPrompt::HostKeyConfirmation { .. }));
    assert!(matches!(asked[1], AskpassPrompt::Password { .. }));
    let known = std::fs::read_to_string(&fixture.known_hosts).unwrap();
    assert!(known.contains(&format!("]:{} ", fixture.port)));
    session.shutdown().await;
    assert!(session.is_closed());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn openssh_unknown_host_declined_is_host_key_unknown_declined() {
    let fixture = start().await.unwrap();
    let (session, _) = connect(&fixture, &config(&fixture, password("s3cret")), Vec::new()).await;
    assert!(matches!(
        session.unwrap_err(),
        OpenSshError::HostKeyUnknown { declined: true, .. }
    ));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn openssh_wrong_password_is_authentication_rejected() {
    let fixture = start().await.unwrap();
    let (session, _) = connect(
        &fixture,
        &config(&fixture, password("wrong")),
        vec![PromptAnswer::Accept],
    )
    .await;
    assert!(matches!(
        session.unwrap_err(),
        OpenSshError::AuthenticationRejected { .. }
    ));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn openssh_stored_password_is_not_sent_to_a_jump_host() {
    let fixture = start().await.unwrap();
    let mut through_jump = config(&fixture, password("s3cret"));
    through_jump.destination = SshDestination::new("127.0.0.1", Some(2222), Some("deploy".to_owned())).unwrap();
    through_jump.jump_hosts =
        vec![SshDestination::new(&fixture.host, Some(fixture.port), Some("ops".to_owned())).unwrap()];
    let (session, prompter) = connect(&fixture, &through_jump, vec![PromptAnswer::Accept]).await;

    assert!(session.is_err());
    let ops_prompt = format!("ops@{}'s password: ", fixture.host);
    assert!(
        prompter
            .asked()
            .iter()
            .any(|prompt| matches!(prompt, AskpassPrompt::Password { user_host } if format!("{user_host}'s password: ") == ops_prompt)),
        "{:?}",
        prompter.asked()
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn openssh_forward_reaches_sshd_and_socket_is_0600() {
    let fixture = start().await.unwrap();
    let (session, _) = connect(
        &fixture,
        &config(&fixture, password("s3cret")),
        vec![PromptAnswer::Accept],
    )
    .await;
    let session = session.unwrap();

    let target = ForwardTarget::new("127.0.0.1", 2222).unwrap();
    let forward = session.forward(target, ForwardRoute::UnixSocket).await.unwrap();
    let LocalEndpoint::Unix(socket) = forward.local_endpoint().clone() else {
        panic!("a unix socket forward routes through a unix socket");
    };
    assert_eq!(std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777, 0o600);
    let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    let mut banner = [0u8; 8];
    stream.read_exact(&mut banner).await.unwrap();
    assert_eq!(&banner, b"SSH-2.0-");

    let target = ForwardTarget::new("127.0.0.1", 2222).unwrap();
    let tcp = session.forward(target, ForwardRoute::LoopbackTcp).await.unwrap();
    let LocalEndpoint::Tcp(address) = tcp.local_endpoint().clone() else {
        panic!("a loopback forward routes through a port");
    };
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream.read_exact(&mut banner).await.unwrap();
    assert_eq!(&banner, b"SSH-2.0-");

    drop(stream);
    drop(forward);
    drop(tcp);
    session.shutdown().await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn openssh_killing_master_fires_closed() {
    let fixture = start().await.unwrap();
    let (session, _) = connect(
        &fixture,
        &config(&fixture, password("s3cret")),
        vec![PromptAnswer::Accept],
    )
    .await;
    let session = session.unwrap();
    let pid = session.master_pid().unwrap();

    let killed = std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()
        .unwrap();
    assert!(killed.success());
    tokio::time::timeout(Duration::from_secs(10), session.closed())
        .await
        .unwrap();
    assert!(session.is_closed());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn openssh_dropping_last_arc_ends_master() {
    let fixture = start().await.unwrap();
    let (session, _) = connect(
        &fixture,
        &config(&fixture, password("s3cret")),
        vec![PromptAnswer::Accept],
    )
    .await;
    let session = session.unwrap();
    let pid = session.master_pid().unwrap();

    drop(session);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while process_alive(pid) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!process_alive(pid));
}
