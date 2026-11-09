#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use bookie_ssh::{AlgorithmClass, SshAuth, SshConfig, SshError, SshTunnel, UnknownHostKey};
use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};
use russh::server::{self, Config};
use russh::{Preferred, cipher, kex, mac};
use tokio::net::TcpListener;

struct Peer;

impl server::Handler for Peer {
    type Error = russh::Error;
}

fn host_key() -> PrivateKey {
    PrivateKey::new(
        KeypairData::Ed25519(Ed25519Keypair::from_seed(&[7; 32])),
        "algorithm policy test peer",
    )
    .unwrap()
}

fn peer_config(preferred: Preferred) -> Config {
    Config {
        keys: vec![host_key()],
        preferred,
        ..Config::default()
    }
}

async fn start_peer(config: Config) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(config);
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
    port
}

async fn open_against(preferred: Preferred) -> SshError {
    open_config(peer_config(preferred)).await
}

async fn open_config(peer: Config) -> SshError {
    let port = start_peer(peer).await;
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
}

fn refused(error: SshError) -> (AlgorithmClass, String) {
    match error {
        SshError::NoAcceptableAlgorithm { class, offered } => (class, offered),
        other => panic!("expected a named algorithm refusal, got {other}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_modern_peer_passes_negotiation_and_reaches_the_host_key_check() {
    let error = open_against(Preferred::DEFAULT).await;
    assert!(matches!(error, SshError::UnknownHostKey { .. }), "{error}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_peer_offering_only_sha1_key_exchange_is_refused_by_name() {
    let (class, offered) = refused(
        open_against(Preferred {
            kex: Cow::Borrowed(&[kex::DH_G14_SHA1]),
            ..Preferred::DEFAULT
        })
        .await,
    );
    assert_eq!(class, AlgorithmClass::KeyExchange);
    assert!(offered.contains("diffie-hellman-group14-sha1"), "{offered}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_peer_offering_only_a_cbc_cipher_is_refused_by_name() {
    let (class, offered) = refused(
        open_against(Preferred {
            cipher: Cow::Borrowed(&[cipher::AES_128_CBC]),
            ..Preferred::DEFAULT
        })
        .await,
    );
    assert_eq!(class, AlgorithmClass::Cipher);
    assert!(offered.contains("aes128-cbc"), "{offered}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_peer_offering_only_a_sha1_mac_is_refused_by_name() {
    let (class, offered) = refused(
        open_against(Preferred {
            cipher: Cow::Borrowed(&[cipher::AES_256_CTR]),
            mac: Cow::Borrowed(&[mac::HMAC_SHA1]),
            ..Preferred::DEFAULT
        })
        .await,
    );
    assert_eq!(class, AlgorithmClass::Mac);
    assert!(offered.contains("hmac-sha1"), "{offered}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_peer_offering_only_a_sha1_rsa_host_key_is_refused_by_name() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("host_rsa");
    let status = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "rsa", "-b", "2048", "-N", "", "-f"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    let peer = Config {
        keys: vec![PrivateKey::read_openssh_file(path).unwrap()],
        preferred: Preferred {
            key: Cow::Owned(vec![russh::keys::Algorithm::Rsa { hash: None }]),
            ..Preferred::DEFAULT
        },
        ..Config::default()
    };
    let (class, offered) = refused(open_config(peer).await);
    assert_eq!(class, AlgorithmClass::HostKey);
    assert!(offered.contains("ssh-rsa"), "{offered}");
}
