use std::collections::HashMap;
use std::sync::Mutex;

use bookie_ssh::SshAuth;
use bookie_storage::{SavedSshAuth, SavedSshConfig, SshClient, StorageError};
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use super::*;
use crate::ssh_chain_form::{HopSecretKind, HopSecretRef, JumpAuth, JumpHopForm, plan_chain};

#[derive(Default)]
struct FakeStore {
    entries: Mutex<HashMap<(Uuid, u64, bool), String>>,
}

impl HopSecretStore for FakeStore {
    async fn store(&self, _: Uuid, _: HopSecretRef, _: &SecretString, _: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _: Uuid, _: HopSecretRef) -> Result<(), StorageError> {
        Ok(())
    }

    async fn load(&self, _: Uuid, secret: HopSecretRef) -> Result<Option<SecretString>, StorageError> {
        let key = (secret.hop_id, secret.revision, secret.kind == HopSecretKind::Password);
        Ok(self
            .entries
            .lock()
            .unwrap()
            .get(&key)
            .map(|value| SecretString::new(value.clone().into())))
    }
}

fn head() -> SavedSshConfig {
    SavedSshConfig {
        hop_id: Uuid::new_v4(),
        credential_revision: 0,
        host: "edge".into(),
        port: 22,
        username: "me".into(),
        auth: SavedSshAuth::Password,
        jump: None,
        client: SshClient::Builtin,
        agent: false,
        agent_identity: None,
        local_port: None,
    }
}

fn password_form(hop_id: Option<Uuid>, password: Option<&str>, stored: bool) -> JumpHopForm {
    JumpHopForm {
        hop_id,
        host: "inner".into(),
        port: 2222,
        username: "ops".into(),
        auth: JumpAuth::Password,
        key_path: String::new(),
        agent_identity: String::new(),
        password: password.map(|value| SecretString::new(value.into())),
        passphrase: None,
        stored_password: stored,
        stored_passphrase: false,
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
fn an_entered_secret_resolves_without_touching_the_keyring() {
    let plan = plan_chain(head(), &[password_form(None, Some("typed"), false)], None).unwrap();
    let hops = block_on(resolve_with(&FakeStore::default(), None, &plan)).unwrap();
    assert_eq!((hops[0].host.as_str(), hops[0].port), ("inner", 2222));
    assert!(matches!(&hops[0].auth, SshAuth::Password { password } if password.expose_secret() == "typed"));
}

#[test]
fn a_stored_secret_is_loaded_for_the_hop_it_belongs_to() {
    let hop_id = Uuid::new_v4();
    let mut previous = head();
    previous.jump = Some(Box::new(SavedSshConfig {
        hop_id,
        credential_revision: 2,
        ..head()
    }));
    let plan = plan_chain(
        previous.clone(),
        &[password_form(Some(hop_id), None, true)],
        Some(&previous),
    )
    .unwrap();
    let connection = Uuid::new_v4();
    let store = FakeStore::default();
    store
        .entries
        .lock()
        .unwrap()
        .insert((hop_id, 2, true), "from-keyring".into());
    let hops = block_on(resolve_with(&store, Some(connection), &plan)).unwrap();
    assert!(matches!(&hops[0].auth, SshAuth::Password { password } if password.expose_secret() == "from-keyring"));
}

#[test]
fn a_secret_missing_from_the_keyring_is_reported_with_its_hop_number() {
    let hop_id = Uuid::new_v4();
    let mut previous = head();
    previous.jump = Some(Box::new(SavedSshConfig {
        hop_id,
        credential_revision: 2,
        ..head()
    }));
    let plan = plan_chain(
        previous.clone(),
        &[password_form(Some(hop_id), None, true)],
        Some(&previous),
    )
    .unwrap();
    let error = block_on(resolve_with(&FakeStore::default(), Some(Uuid::new_v4()), &plan)).unwrap_err();
    assert!(matches!(error, ChainConnectError::SecretMissing { position: 2 }));
    let unbound = block_on(resolve_with(&FakeStore::default(), None, &plan)).unwrap_err();
    assert!(matches!(unbound, ChainConnectError::SecretMissing { position: 2 }));
}
