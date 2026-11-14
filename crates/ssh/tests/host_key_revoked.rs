#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use bookie_ssh::{SshAuth, SshConfig, SshError, SshTunnel, UnknownHostKey, default_known_hosts_path};
use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};
use russh::server::{self, Config};
use tokio::net::TcpListener;

struct Peer;

impl server::Handler for Peer {
    type Error = russh::Error;
}

fn ecdsa_host_key(dir: &std::path::Path) -> PrivateKey {
    let path = dir.join("host_ecdsa");
    let status = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ecdsa", "-b", "256", "-N", "", "-f"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    PrivateKey::read_openssh_file(&path).unwrap()
}

#[test]
fn a_revoked_host_key_is_refused_as_revoked_not_as_a_known_hosts_fault() {
    let temp = tempfile::tempdir().unwrap();
    // SAFETY: this test file holds a single test and sets the variable before any other thread exists.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", temp.path()) };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    let ecdsa = ecdsa_host_key(temp.path());
    let ed25519 = PrivateKey::new(
        KeypairData::Ed25519(Ed25519Keypair::from_seed(&[7; 32])),
        "host key revoked test peer",
    )
    .unwrap();
    let ed25519_public = ed25519.public_key().to_openssh().unwrap();
    let error = runtime.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = Arc::new(Config {
            keys: vec![ed25519, ecdsa.clone()],
            ..Config::default()
        });
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let config = config.clone();
                tokio::spawn(async move {
                    if let Ok(session) = server::run_stream(config, stream, Peer).await {
                        let _ = session.await;
                    }
                });
            }
        });

        let known_hosts = default_known_hosts_path().unwrap();
        std::fs::create_dir_all(known_hosts.parent().unwrap()).unwrap();
        std::fs::write(
            &known_hosts,
            format!("@revoked [127.0.0.1]:{port} {}\n", ed25519_public),
        )
        .unwrap();

        let config = SshConfig {
            host: "127.0.0.1".into(),
            port,
            username: "tunnel".into(),
            auth: SshAuth::PrivateKey {
                path: PathBuf::from("/nonexistent/id_ed25519"),
                passphrase: None,
            },
            local_port: None,
        };
        match SshTunnel::open(config, "db.example".into(), 5432, UnknownHostKey::Refuse).await {
            Ok(_) => panic!("the test peer must never yield a usable tunnel"),
            Err(error) => error,
        }
    });

    assert!(
        matches!(error, SshError::HostKeyRevoked(_)),
        "a revoked key must be reported as revoked, got: {error:?}"
    );
}
