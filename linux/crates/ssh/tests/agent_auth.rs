#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

use tablepro_ssh::{SshAuth, SshConfig, SshError, SshTunnel, UnknownHostKey};
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

const SSHD_SCRIPT: &str = "apk add --no-cache openssh >/dev/null \
    && ssh-keygen -A >/dev/null \
    && adduser -D deploy && echo 'deploy:unused' | chpasswd \
    && mkdir -p /home/deploy/.ssh && printf '%s\\n' \"$AUTHORIZED_KEY\" > /home/deploy/.ssh/authorized_keys \
    && chown -R deploy /home/deploy/.ssh && chmod 700 /home/deploy/.ssh && chmod 600 /home/deploy/.ssh/authorized_keys \
    && exec /usr/sbin/sshd -D -e -p 2222 -o PasswordAuthentication=no -o AllowTcpForwarding=yes";

struct Agent {
    pid: String,
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = Command::new("kill").arg(&self.pid).status();
    }
}

fn start_agent(socket: &std::path::Path) -> Agent {
    let output = Command::new("ssh-agent")
        .arg("-a")
        .arg(socket)
        .arg("-s")
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let pid = text
        .split(';')
        .find_map(|part| part.trim().strip_prefix("SSH_AGENT_PID="))
        .expect("ssh-agent printed its pid")
        .to_string();
    Agent { pid }
}

fn config(host: &str, port: u16) -> SshConfig {
    SshConfig {
        host: host.to_string(),
        port,
        username: "deploy".into(),
        auth: SshAuth::Agent,
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn the_built_in_client_authenticates_with_a_key_held_by_ssh_agent() {
    let temp = tempfile::tempdir().unwrap();
    let key = temp.path().join("id_ed25519");
    let generated = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&key)
        .status()
        .unwrap();
    assert!(generated.success());
    let public = std::fs::read_to_string(key.with_extension("pub")).unwrap();

    let container = GenericImage::new("alpine", "3.22")
        .with_exposed_port(2222.tcp())
        .with_wait_for(WaitFor::message_on_stderr("Server listening on"))
        .with_env_var("AUTHORIZED_KEY", public.trim())
        .with_cmd(["sh", "-c", SSHD_SCRIPT])
        .start()
        .await
        .unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(2222).await.unwrap();

    let socket = temp.path().join("agent.sock");
    let _agent = start_agent(&socket);
    // SAFETY: this binary holds a single test, so nothing else reads the
    // environment while it changes. russh finds the agent through
    // SSH_AUTH_SOCK, and known_hosts is resolved from XDG_CONFIG_HOME.
    unsafe {
        std::env::set_var("SSH_AUTH_SOCK", &socket);
        std::env::set_var("XDG_CONFIG_HOME", temp.path());
    }

    let empty = SshTunnel::open(config(&host, port), "127.0.0.1".into(), 2222, UnknownHostKey::Learn).await;
    assert!(
        matches!(&empty, Err(SshError::Agent(message)) if message.contains("no keys")),
        "{:?}",
        empty.as_ref().err()
    );

    let added = Command::new("ssh-add")
        .arg(&key)
        .env("SSH_AUTH_SOCK", &socket)
        .status()
        .unwrap();
    assert!(added.success());
    let tunnel = SshTunnel::open(config(&host, port), "127.0.0.1".into(), 2222, UnknownHostKey::Learn).await;
    assert!(tunnel.is_ok(), "{:?}", tunnel.err());
}
