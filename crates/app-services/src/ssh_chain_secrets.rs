use bookie_storage::StorageError;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use uuid::Uuid;

use crate::ssh_chain_form::{ChainPlan, HopSecretKind, HopSecretRef};

#[derive(Debug, Error)]
pub enum HopSecretError {
    #[error("jump host secret storage failed: {0}")]
    Storage(#[from] StorageError),
}

#[derive(Debug, Default)]
pub struct CommittedHopSecrets {
    written: Vec<HopSecretRef>,
}

impl CommittedHopSecrets {
    pub fn written(&self) -> &[HopSecretRef] {
        &self.written
    }
}

pub(crate) trait HopSecretStore {
    async fn store(
        &self,
        connection_id: Uuid,
        secret: HopSecretRef,
        value: &SecretString,
        label: &str,
    ) -> Result<(), StorageError>;
    async fn delete(&self, connection_id: Uuid, secret: HopSecretRef) -> Result<(), StorageError>;
    async fn load(&self, connection_id: Uuid, secret: HopSecretRef) -> Result<Option<SecretString>, StorageError>;
}

pub(crate) struct SecretServiceStore;

impl HopSecretStore for SecretServiceStore {
    async fn store(
        &self,
        connection_id: Uuid,
        secret: HopSecretRef,
        value: &SecretString,
        label: &str,
    ) -> Result<(), StorageError> {
        let HopSecretRef { hop_id, revision, kind } = secret;
        match kind {
            HopSecretKind::Password => {
                bookie_storage::store_ssh_hop_password(connection_id, hop_id, revision, value.expose_secret(), label)
                    .await
            }
            HopSecretKind::Passphrase => {
                bookie_storage::store_ssh_hop_passphrase(connection_id, hop_id, revision, value.expose_secret(), label)
                    .await
            }
        }
    }

    async fn delete(&self, connection_id: Uuid, secret: HopSecretRef) -> Result<(), StorageError> {
        let HopSecretRef { hop_id, revision, kind } = secret;
        match kind {
            HopSecretKind::Password => bookie_storage::delete_ssh_hop_password(connection_id, hop_id, revision).await,
            HopSecretKind::Passphrase => {
                bookie_storage::delete_ssh_hop_passphrase(connection_id, hop_id, revision).await
            }
        }
    }

    async fn load(&self, connection_id: Uuid, secret: HopSecretRef) -> Result<Option<SecretString>, StorageError> {
        let HopSecretRef { hop_id, revision, kind } = secret;
        match kind {
            HopSecretKind::Password => bookie_storage::load_ssh_hop_password(connection_id, hop_id, revision).await,
            HopSecretKind::Passphrase => bookie_storage::load_ssh_hop_passphrase(connection_id, hop_id, revision).await,
        }
    }
}

pub async fn commit_hop_secrets(
    connection_id: Uuid,
    plan: &ChainPlan,
    label: &str,
) -> Result<CommittedHopSecrets, HopSecretError> {
    commit_with(&SecretServiceStore, connection_id, plan, label).await
}

pub async fn rollback_hop_secrets(connection_id: Uuid, committed: &CommittedHopSecrets) -> Result<(), HopSecretError> {
    delete_all(&SecretServiceStore, connection_id, &committed.written).await
}

pub async fn delete_obsolete_hop_secrets(connection_id: Uuid, plan: &ChainPlan) -> Result<(), HopSecretError> {
    delete_all(&SecretServiceStore, connection_id, &plan.obsolete).await
}

pub async fn load_hop_secret(
    connection_id: Uuid,
    secret: HopSecretRef,
) -> Result<Option<SecretString>, HopSecretError> {
    Ok(SecretServiceStore.load(connection_id, secret).await?)
}

pub async fn hop_secret_is_stored(connection_id: Uuid, secret: HopSecretRef) -> Result<bool, HopSecretError> {
    Ok(SecretServiceStore.load(connection_id, secret).await?.is_some())
}

async fn commit_with<S: HopSecretStore>(
    store: &S,
    connection_id: Uuid,
    plan: &ChainPlan,
    label: &str,
) -> Result<CommittedHopSecrets, HopSecretError> {
    let mut committed = CommittedHopSecrets::default();
    for write in &plan.writes {
        if let Err(error) = store.store(connection_id, write.secret, &write.value, label).await {
            let _ = delete_all(store, connection_id, &committed.written).await;
            return Err(error.into());
        }
        committed.written.push(write.secret);
    }
    Ok(committed)
}

async fn delete_all<S: HopSecretStore>(
    store: &S,
    connection_id: Uuid,
    secrets: &[HopSecretRef],
) -> Result<(), HopSecretError> {
    let mut first_error = None;
    for secret in secrets {
        if let Err(error) = store.delete(connection_id, *secret).await {
            first_error.get_or_insert(error);
        }
    }
    match first_error {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "ssh_chain_secrets_tests.rs"]
mod tests;
