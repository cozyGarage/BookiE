use bookie_storage::{SavedConnection, store_ssh_passphrase, store_ssh_password};
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use super::connect_failure::ConnectFailure;
use crate::ui::ssh_section::{SshInputs, SshSecretToStore};

pub(super) struct PersistRequest<'a> {
    pub saved: &'a SavedConnection,
    pub existing: Option<&'a SavedConnection>,
    pub stored_password: &'a SecretString,
    pub label: &'a str,
    pub ssh: Option<&'a SshInputs>,
}

pub(super) async fn persist_connection(request: PersistRequest<'_>) -> Result<(), ConnectFailure> {
    let PersistRequest {
        saved,
        existing,
        stored_password,
        label,
        ssh,
    } = request;
    let committed = match ssh {
        Some(inputs) => Some(
            bookie_app_services::ssh_chain_secrets::commit_hop_secrets(saved.id, &inputs.chain, label)
                .await
                .map_err(|error| format!("The SSH jump host secrets were not saved: {error}"))?,
        ),
        None => None,
    };
    if let Err(error) = save_and_store_password(saved, existing, stored_password, label).await {
        if let Some(committed) = &committed {
            discard_hop_secrets(saved.id, committed).await;
        }
        return Err(error);
    }
    if let Some(inputs) = ssh {
        store_head_secret(saved.id, inputs, label).await?;
        forget_obsolete_hop_secrets(saved.id, inputs).await;
    }
    Ok(())
}

async fn save_and_store_password(
    saved: &SavedConnection,
    existing: Option<&SavedConnection>,
    stored_password: &SecretString,
    label: &str,
) -> Result<(), ConnectFailure> {
    bookie_app_services::connection_save::persist_with_password(saved, existing, stored_password, label)
        .await
        .map_err(ConnectFailure::from)
}

async fn store_head_secret(id: Uuid, inputs: &SshInputs, label: &str) -> Result<(), ConnectFailure> {
    match &inputs.secret_to_store {
        SshSecretToStore::Password(password) => {
            store_ssh_password(id, password.expose_secret(), label).await.map_err(|error| {
                format!("Connection settings were saved, but the SSH password was not saved: {error}. Re-enter the SSH password and connect again.")
            })?;
        }
        SshSecretToStore::Passphrase(passphrase) => {
            store_ssh_passphrase(id, passphrase.expose_secret(), label).await.map_err(|error| {
                format!("Connection settings were saved, but the SSH key passphrase was not saved: {error}. Re-enter the passphrase and connect again.")
            })?;
        }
        SshSecretToStore::None => {}
    }
    Ok(())
}

async fn discard_hop_secrets(id: Uuid, committed: &bookie_app_services::ssh_chain_secrets::CommittedHopSecrets) {
    if let Err(error) = bookie_app_services::ssh_chain_secrets::rollback_hop_secrets(id, committed).await {
        tracing::warn!(%error, "jump host secrets from a failed save could not be removed");
    }
}

async fn forget_obsolete_hop_secrets(id: Uuid, inputs: &SshInputs) {
    if let Err(error) = bookie_app_services::ssh_chain_secrets::delete_obsolete_hop_secrets(id, &inputs.chain).await {
        tracing::warn!(%error, "replaced jump host secrets could not be removed");
    }
}
