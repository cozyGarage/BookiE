use std::collections::HashMap;
use std::sync::Mutex;

use bookie_storage::{KeyringFailure, SavedSshAuth, SavedSshConfig, SshClient};
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use super::*;
use crate::ssh_chain_form::{JumpAuth, JumpHopForm, plan_chain};

#[derive(Default)]
struct FakeStore {
    entries: Mutex<HashMap<(Uuid, u64, bool), String>>,
    fail_on_store: Mutex<Option<usize>>,
    stores: Mutex<usize>,
}

impl FakeStore {
    fn key(secret: HopSecretRef) -> (Uuid, u64, bool) {
        (secret.hop_id, secret.revision, secret.kind == HopSecretKind::Password)
    }

    fn holds(&self, secret: HopSecretRef) -> bool {
        self.entries.lock().unwrap().contains_key(&Self::key(secret))
    }

    fn seed(&self, secret: HopSecretRef, value: &str) {
        self.entries.lock().unwrap().insert(Self::key(secret), value.into());
    }
}

impl HopSecretStore for FakeStore {
    async fn store(&self, _: Uuid, secret: HopSecretRef, value: &SecretString, _: &str) -> Result<(), StorageError> {
        let mut stores = self.stores.lock().unwrap();
        *stores += 1;
        if *self.fail_on_store.lock().unwrap() == Some(*stores) {
            return Err(StorageError::Keyring(KeyringFailure::Unavailable));
        }
        self.entries
            .lock()
            .unwrap()
            .insert(Self::key(secret), value.expose_secret().into());
        Ok(())
    }

    async fn delete(&self, _: Uuid, secret: HopSecretRef) -> Result<(), StorageError> {
        self.entries.lock().unwrap().remove(&Self::key(secret));
        Ok(())
    }

    async fn load(&self, _: Uuid, secret: HopSecretRef) -> Result<Option<SecretString>, StorageError> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .get(&Self::key(secret))
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

fn password_hop(host: &str, password: &str) -> JumpHopForm {
    JumpHopForm {
        hop_id: None,
        host: host.into(),
        port: 22,
        username: "me".into(),
        auth: JumpAuth::Password,
        key_path: String::new(),
        agent_identity: String::new(),
        password: Some(SecretString::new(password.into())),
        passphrase: None,
        stored_password: false,
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
fn committed_hop_secrets_are_stored_under_each_hops_identity_and_revision() {
    let plan = plan_chain(head(), &[password_hop("a", "one"), password_hop("b", "two")], None).unwrap();
    let store = FakeStore::default();
    let committed = block_on(commit_with(&store, Uuid::new_v4(), &plan, "db")).unwrap();
    assert_eq!(committed.written().len(), 2);
    for write in &plan.writes {
        assert!(store.holds(write.secret));
        assert_eq!(write.secret.revision, 1);
    }
}

#[test]
fn a_failed_second_write_removes_the_first_and_reports_the_failure() {
    let plan = plan_chain(head(), &[password_hop("a", "one"), password_hop("b", "two")], None).unwrap();
    let store = FakeStore::default();
    *store.fail_on_store.lock().unwrap() = Some(2);
    let result = block_on(commit_with(&store, Uuid::new_v4(), &plan, "db"));
    assert!(result.is_err());
    assert!(plan.writes.iter().all(|write| !store.holds(write.secret)));
}

#[test]
fn rolling_back_removes_what_was_written_and_nothing_else() {
    let plan = plan_chain(head(), &[password_hop("a", "one")], None).unwrap();
    let store = FakeStore::default();
    let unrelated = HopSecretRef {
        hop_id: Uuid::new_v4(),
        revision: 1,
        kind: HopSecretKind::Password,
    };
    store.seed(unrelated, "keep");
    let committed = block_on(commit_with(&store, Uuid::new_v4(), &plan, "db")).unwrap();
    block_on(delete_all(&store, Uuid::new_v4(), committed.written())).unwrap();
    assert!(plan.writes.iter().all(|write| !store.holds(write.secret)));
    assert!(store.holds(unrelated));
}

#[test]
fn the_secrets_of_a_removed_hop_and_a_replaced_revision_are_deleted_after_a_save() {
    let first = plan_chain(head(), &[password_hop("a", "one"), password_hop("b", "two")], None).unwrap();
    let store = FakeStore::default();
    let connection = Uuid::new_v4();
    block_on(commit_with(&store, connection, &first, "db")).unwrap();
    let saved = first.head.clone();
    let hops = saved.flatten_hops();
    let kept = JumpHopForm {
        hop_id: Some(hops[1].hop_id),
        password: Some(SecretString::new("rotated".into())),
        ..password_hop("a", "")
    };
    let second = plan_chain(saved.clone(), &[kept], Some(&saved)).unwrap();
    let committed = block_on(commit_with(&store, connection, &second, "db")).unwrap();
    block_on(delete_all(&store, connection, &second.obsolete)).unwrap();
    assert!(committed.written().iter().all(|secret| store.holds(*secret)));
    for write in &first.writes {
        assert!(!store.holds(write.secret));
    }
}

#[test]
fn a_stored_secret_loads_back_for_a_connect() {
    let plan = plan_chain(head(), &[password_hop("a", "one")], None).unwrap();
    let store = FakeStore::default();
    let connection = Uuid::new_v4();
    block_on(commit_with(&store, connection, &plan, "db")).unwrap();
    let loaded = block_on(store.load(connection, plan.writes[0].secret)).unwrap();
    assert_eq!(loaded.unwrap().expose_secret(), "one");
}

#[test]
fn deleting_a_connection_removes_the_secrets_of_every_hop_in_its_chain() {
    let plan = plan_chain(head(), &[password_hop("a", "one"), password_hop("b", "two")], None).unwrap();
    let store = FakeStore::default();
    let connection = Uuid::new_v4();
    block_on(commit_with(&store, connection, &plan, "db")).unwrap();
    let refs = chain_secret_refs(&plan.head);
    assert_eq!(refs.len(), 2);
    assert!(refs.iter().all(|secret| store.holds(*secret)));
    block_on(delete_all(&store, connection, &refs)).unwrap();
    assert!(refs.iter().all(|secret| !store.holds(*secret)));
}

#[test]
fn a_chain_without_hop_secrets_has_nothing_to_delete() {
    let plan = plan_chain(head(), &[], None).unwrap();
    assert!(chain_secret_refs(&plan.head).is_empty());
}

#[test]
#[ignore = "requires a running Secret Service; scripts/test-secret-service.sh provides one"]
fn jump_host_secrets_round_trip_through_the_secret_service_and_clear_on_obsolescence() {
    let connection = Uuid::new_v4();
    let first = plan_chain(head(), &[password_hop("a", "one"), password_hop("b", "two")], None).unwrap();
    block_on(async {
        let committed = commit_hop_secrets(connection, &first, "ssh chain test").await.unwrap();
        assert_eq!(committed.written().len(), 2);
        for write in &first.writes {
            let loaded = load_hop_secret(connection, write.secret).await.unwrap().unwrap();
            assert_eq!(loaded.expose_secret(), write.value.expose_secret());
        }
        let saved = first.head.clone();
        let second = plan_chain(saved.clone(), &[], Some(&saved)).unwrap();
        delete_obsolete_hop_secrets(connection, &second).await.unwrap();
        for write in &first.writes {
            assert!(!hop_secret_is_stored(connection, write.secret).await.unwrap());
        }
        rollback_hop_secrets(connection, &committed).await.unwrap();
    });
}
