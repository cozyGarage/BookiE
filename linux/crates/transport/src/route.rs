use std::path::Path;
use std::sync::Arc;

use tablepro_ssh::openssh::{
    ForwardRoute, ForwardTarget, LocalEndpoint, OpenSshAuth, OpenSshConfig, OpenSshContext, OpenSshForward,
    OpenSshSession, Prompter, SshDestination,
};
use tablepro_ssh::{SshAuth, SshConfig, SshTunnel, UnknownHostKey};
use tablepro_storage::SavedSshConfig;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::TransportError;

#[derive(Clone)]
pub enum SshRoute {
    Builtin(Vec<SshConfig>),
    OpenSsh(OpenSshConfig),
}

pub enum Tunnel {
    Builtin(SshTunnel),
    OpenSsh(OpenSshForward),
}

impl Tunnel {
    pub fn socket_dir(&self) -> Option<&Path> {
        match self {
            Self::Builtin(tunnel) => tunnel.socket_dir(),
            Self::OpenSsh(forward) => match forward.local_endpoint() {
                LocalEndpoint::Unix(path) => path.parent(),
                LocalEndpoint::Tcp(_) => None,
            },
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            Self::Builtin(tunnel) => tunnel.is_closed(),
            Self::OpenSsh(forward) => forward.session().is_closed(),
        }
    }
}

#[derive(Clone)]
pub struct OpenSshEnvironment {
    pub context: OpenSshContext,
    pub prompter: Arc<dyn Prompter>,
}

#[derive(Clone)]
pub struct SshEnvironment {
    pub unknown_host_key: UnknownHostKey,
    pub openssh: Option<OpenSshEnvironment>,
}

impl SshEnvironment {
    pub fn builtin(unknown_host_key: UnknownHostKey) -> Self {
        Self {
            unknown_host_key,
            openssh: None,
        }
    }
}

pub(crate) async fn openssh_config(id: Uuid, saved: &SavedSshConfig) -> Result<OpenSshConfig, TransportError> {
    if saved.jump.is_some() {
        return Err(TransportError::Ssh(
            "the system OpenSSH client takes jump hosts from ~/.ssh/config (ProxyJump); remove the saved jump \
             hops or switch this connection to the built-in SSH client"
                .into(),
        ));
    }
    openssh_config_for(&crate::resolve_saved_ssh_hop(id, saved, 0).await?)
}

pub fn openssh_config_for(config: &SshConfig) -> Result<OpenSshConfig, TransportError> {
    let destination = SshDestination::new(&config.host, Some(config.port), Some(config.username.clone()))
        .map_err(|error| TransportError::Ssh(error.to_string()))?;
    let auth = match &config.auth {
        SshAuth::Password { password } => OpenSshAuth::Password {
            password: password.clone(),
        },
        SshAuth::PrivateKey { path, passphrase } => OpenSshAuth::PrivateKey {
            path: Some(path.clone()),
            passphrase: passphrase.clone(),
        },
        SshAuth::Agent => OpenSshAuth::Agent,
    };
    Ok(OpenSshConfig {
        destination,
        jump_hosts: Vec::new(),
        auth,
    })
}

pub(crate) async fn open_openssh(
    config: &OpenSshConfig,
    environment: &SshEnvironment,
    remote: (&str, u16),
    socket_name: Option<String>,
    cancel: CancellationToken,
) -> Result<OpenSshForward, TransportError> {
    let openssh = environment
        .openssh
        .as_ref()
        .ok_or_else(|| TransportError::Ssh("the system OpenSSH client is not available in this process".into()))?;
    let ssh = |error: tablepro_ssh::openssh::OpenSshError| TransportError::Ssh(error.to_string());
    let session = OpenSshSession::connect(config, &openssh.context, openssh.prompter.clone(), cancel)
        .await
        .map_err(ssh)?;
    let target = ForwardTarget::new(remote.0, remote.1).map_err(ssh)?;
    let route = match socket_name {
        Some(name) => ForwardRoute::NamedUnixSocket(name),
        None => ForwardRoute::LoopbackTcp,
    };
    session.forward(target, route).await.map_err(ssh)
}

const ASKPASS_PROGRAM: &str = "tablepro-askpass";

pub async fn system_openssh(prompter: Arc<dyn Prompter>) -> Result<OpenSshEnvironment, TransportError> {
    let ssh = |error: tablepro_ssh::openssh::OpenSshError| TransportError::Ssh(error.to_string());
    let ssh_program = find_in_path("ssh", std::env::var_os("PATH").as_deref())
        .ok_or_else(|| TransportError::Ssh("the ssh program was not found on PATH".into()))?;
    let askpass_program = beside_current_exe(ASKPASS_PROGRAM)
        .or_else(|| find_in_path(ASKPASS_PROGRAM, std::env::var_os("PATH").as_deref()))
        .ok_or_else(|| TransportError::Ssh(format!("the {ASKPASS_PROGRAM} helper is not installed")))?;
    let base = tablepro_ssh::openssh::OpenSshRuntime::default_base().map_err(ssh)?;
    let runtime = tablepro_ssh::openssh::OpenSshRuntime::acquire(&base).map_err(ssh)?;
    tablepro_ssh::openssh::sweep_stale_masters(&runtime, &ssh_program).await;
    Ok(OpenSshEnvironment {
        context: OpenSshContext {
            runtime,
            ssh_program,
            askpass_program,
            timeouts: Default::default(),
        },
        prompter,
    })
}

fn beside_current_exe(name: &str) -> Option<std::path::PathBuf> {
    let candidate = std::env::current_exe().ok()?.parent()?.join(name);
    is_executable(&candidate).then_some(candidate)
}

fn find_in_path(name: &str, path: Option<&std::ffi::OsStr>) -> Option<std::path::PathBuf> {
    std::env::split_paths(path?)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_program_is_found_only_in_absolute_path_entries_and_only_when_executable() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("ssh");
        std::fs::write(&program, "#!/bin/sh\n").unwrap();
        let path = std::env::join_paths(["relative/bin".into(), directory.path().to_path_buf()]).unwrap();

        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(find_in_path("ssh", Some(&path)), None);

        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(find_in_path("ssh", Some(&path)), Some(program));
        assert_eq!(find_in_path("ssh", None), None);
    }

    const FAKE_SSH_THAT_HANGS_WHILE_PROMPTING: &str = r#"#!/bin/sh
dir='@DIR@'
case " $* " in
  *" -O "*) exit 0 ;;
esac
echo "$$" > "$dir/master.pid"
exec sleep 300
"#;

    async fn wait_until_dead(pid: u32) -> bool {
        for _ in 0..100 {
            if !Path::new(&format!("/proc/{pid}")).exists() {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        false
    }

    #[tokio::test]
    async fn cancelling_while_open_openssh_is_connecting_stops_the_master_and_removes_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("ssh");
        std::fs::write(
            &script,
            FAKE_SSH_THAT_HANGS_WHILE_PROMPTING.replace("@DIR@", &dir.path().display().to_string()),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let base = dir.path().join("run");
        std::fs::create_dir(&base).unwrap();
        let runtime = tablepro_ssh::openssh::OpenSshRuntime::acquire(&base).unwrap();
        let context = tablepro_ssh::openssh::OpenSshContext {
            runtime: runtime.clone(),
            ssh_program: script,
            askpass_program: dir.path().join("askpass"),
            timeouts: tablepro_ssh::openssh::OpenSshTimeouts {
                handshake: std::time::Duration::from_secs(60),
                grace: std::time::Duration::from_millis(300),
                ..Default::default()
            },
        };
        let environment = SshEnvironment {
            unknown_host_key: tablepro_ssh::UnknownHostKey::Refuse,
            openssh: Some(OpenSshEnvironment {
                context,
                prompter: Arc::new(tablepro_ssh::openssh::UnattendedPrompter),
            }),
        };
        let config = OpenSshConfig {
            destination: SshDestination::new("bastion", None, None).unwrap(),
            jump_hosts: Vec::new(),
            auth: OpenSshAuth::Agent,
        };

        let cancel = CancellationToken::new();
        let connecting = open_openssh(&config, &environment, ("db.internal", 5432), None, cancel.clone());
        let cancelling = async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            cancel.cancel();
        };
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(connecting, cancelling)
        })
        .await
        .expect("cancellation must stop the connect well before the 60s handshake timeout");
        assert!(result.is_err(), "a cancelled connect must fail");

        let pid: u32 = std::fs::read_to_string(dir.path().join("master.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(wait_until_dead(pid).await, "the master process must stop");

        let leftovers: Vec<_> = std::fs::read_dir(runtime.instance_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name() != "lock")
            .collect();
        assert!(
            leftovers.is_empty(),
            "the master directory must be removed: {leftovers:?}"
        );
    }
}
