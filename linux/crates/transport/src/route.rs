use std::path::Path;
use std::sync::Arc;

use tablepro_ssh::openssh::{
    ForwardRoute, ForwardTarget, LocalEndpoint, OpenSshAuth, OpenSshConfig, OpenSshContext, OpenSshForward,
    OpenSshSession, Prompter, SshDestination,
};
use tablepro_ssh::{SshConfig, SshTunnel, UnknownHostKey};
use tablepro_storage::{SavedSshAuth, SavedSshConfig};
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
    let destination = SshDestination::new(&saved.host, Some(saved.port), Some(saved.username.clone()))
        .map_err(|error| TransportError::Ssh(error.to_string()))?;
    let auth = match &saved.auth {
        SavedSshAuth::Password => OpenSshAuth::Password {
            password: crate::saved_ssh_password(id).await?,
        },
        SavedSshAuth::PrivateKey { path, has_passphrase } => OpenSshAuth::PrivateKey {
            path: Some(path.clone()),
            passphrase: crate::saved_ssh_passphrase(id, *has_passphrase).await?,
        },
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
) -> Result<OpenSshForward, TransportError> {
    let openssh = environment
        .openssh
        .as_ref()
        .ok_or_else(|| TransportError::Ssh("the system OpenSSH client is not available in this process".into()))?;
    let ssh = |error: tablepro_ssh::openssh::OpenSshError| TransportError::Ssh(error.to_string());
    let session = OpenSshSession::connect(
        config,
        &openssh.context,
        openssh.prompter.clone(),
        CancellationToken::new(),
    )
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
}
