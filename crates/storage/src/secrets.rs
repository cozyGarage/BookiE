use std::collections::HashMap;

use oo7::Keyring;
use secrecy::SecretString;
use uuid::Uuid;

use crate::error::StorageError;

const KIND_DB_PASSWORD: &str = "db_password";
const KIND_SSH_PASSWORD: &str = "ssh_password";
const KIND_SSH_PASSPHRASE: &str = "ssh_passphrase";
const KIND_SSH_HOP_PASSWORD: &str = "ssh_hop_password";
const KIND_SSH_HOP_PASSPHRASE: &str = "ssh_hop_passphrase";
const KIND_MCP_TOKEN: &str = "mcp_token";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshHopSecretKind {
    Password,
    Passphrase,
}

impl SshHopSecretKind {
    fn attribute_value(self) -> &'static str {
        match self {
            Self::Password => KIND_SSH_HOP_PASSWORD,
            Self::Passphrase => KIND_SSH_HOP_PASSPHRASE,
        }
    }
}

pub async fn store_password(id: Uuid, password: &str, label: &str) -> Result<(), StorageError> {
    store_secret(id, KIND_DB_PASSWORD, password, label).await
}

pub async fn load_password(id: Uuid) -> Result<Option<SecretString>, StorageError> {
    load_secret(id, KIND_DB_PASSWORD).await
}

pub async fn delete_password(id: Uuid) -> Result<(), StorageError> {
    delete_secret(id, KIND_DB_PASSWORD).await
}

pub async fn store_ssh_password(id: Uuid, password: &str, label: &str) -> Result<(), StorageError> {
    store_secret(id, KIND_SSH_PASSWORD, password, label).await
}

pub async fn load_ssh_password(id: Uuid) -> Result<Option<SecretString>, StorageError> {
    load_secret(id, KIND_SSH_PASSWORD).await
}

pub async fn delete_ssh_password(id: Uuid) -> Result<(), StorageError> {
    delete_secret(id, KIND_SSH_PASSWORD).await
}

pub async fn store_ssh_passphrase(id: Uuid, passphrase: &str, label: &str) -> Result<(), StorageError> {
    store_secret(id, KIND_SSH_PASSPHRASE, passphrase, label).await
}

pub async fn load_ssh_passphrase(id: Uuid) -> Result<Option<SecretString>, StorageError> {
    load_secret(id, KIND_SSH_PASSPHRASE).await
}

pub async fn delete_ssh_passphrase(id: Uuid) -> Result<(), StorageError> {
    delete_secret(id, KIND_SSH_PASSPHRASE).await
}

pub async fn store_ssh_hop_password(
    connection_id: Uuid,
    hop_id: Uuid,
    credential_revision: u64,
    password: &str,
    label: &str,
) -> Result<(), StorageError> {
    store_secret_with_attrs(
        &attrs_for_hop(connection_id, hop_id, credential_revision, SshHopSecretKind::Password),
        password,
        label,
    )
    .await
}

pub async fn load_ssh_hop_password(
    connection_id: Uuid,
    hop_id: Uuid,
    credential_revision: u64,
) -> Result<Option<SecretString>, StorageError> {
    load_secret_with_attrs(&attrs_for_hop(
        connection_id,
        hop_id,
        credential_revision,
        SshHopSecretKind::Password,
    ))
    .await
}

pub async fn delete_ssh_hop_password(
    connection_id: Uuid,
    hop_id: Uuid,
    credential_revision: u64,
) -> Result<(), StorageError> {
    delete_secret_with_attrs(&attrs_for_hop(
        connection_id,
        hop_id,
        credential_revision,
        SshHopSecretKind::Password,
    ))
    .await
}

pub async fn store_ssh_hop_passphrase(
    connection_id: Uuid,
    hop_id: Uuid,
    credential_revision: u64,
    passphrase: &str,
    label: &str,
) -> Result<(), StorageError> {
    store_secret_with_attrs(
        &attrs_for_hop(connection_id, hop_id, credential_revision, SshHopSecretKind::Passphrase),
        passphrase,
        label,
    )
    .await
}

pub async fn load_ssh_hop_passphrase(
    connection_id: Uuid,
    hop_id: Uuid,
    credential_revision: u64,
) -> Result<Option<SecretString>, StorageError> {
    load_secret_with_attrs(&attrs_for_hop(
        connection_id,
        hop_id,
        credential_revision,
        SshHopSecretKind::Passphrase,
    ))
    .await
}

pub async fn delete_ssh_hop_passphrase(
    connection_id: Uuid,
    hop_id: Uuid,
    credential_revision: u64,
) -> Result<(), StorageError> {
    delete_secret_with_attrs(&attrs_for_hop(
        connection_id,
        hop_id,
        credential_revision,
        SshHopSecretKind::Passphrase,
    ))
    .await
}

pub async fn store_mcp_token(id: Uuid, plaintext: &str, label: &str) -> Result<(), StorageError> {
    store_secret(id, KIND_MCP_TOKEN, plaintext, label).await
}

pub async fn load_mcp_token(id: Uuid) -> Result<Option<SecretString>, StorageError> {
    load_secret(id, KIND_MCP_TOKEN).await
}

pub async fn delete_mcp_token(id: Uuid) -> Result<(), StorageError> {
    delete_secret(id, KIND_MCP_TOKEN).await
}

async fn store_secret(id: Uuid, kind: &str, value: &str, label: &str) -> Result<(), StorageError> {
    store_secret_with_attrs(&attrs_for(id, kind), value, label).await
}

async fn store_secret_with_attrs(
    attrs: &HashMap<&'static str, String>,
    value: &str,
    label: &str,
) -> Result<(), StorageError> {
    let keyring = open().await?;
    keyring
        .create_item(label, attrs, value.as_bytes(), true)
        .await
        .map_err(map_err)?;
    Ok(())
}

async fn load_secret(id: Uuid, kind: &str) -> Result<Option<SecretString>, StorageError> {
    load_secret_with_attrs(&attrs_for(id, kind)).await
}

async fn load_secret_with_attrs(attrs: &HashMap<&'static str, String>) -> Result<Option<SecretString>, StorageError> {
    let Some(keyring) = open_without_creating_a_keyring().await? else {
        return Ok(None);
    };
    let items = keyring.search_items(attrs).await.map_err(map_err)?;
    let Some(item) = items.into_iter().next() else {
        return Ok(None);
    };
    let secret = item.secret().await.map_err(map_err)?;
    let s = String::from_utf8(secret.to_vec()).map_err(|e| StorageError::Schema(format!("secret utf8: {e}")))?;
    Ok(Some(SecretString::new(s.into())))
}

async fn delete_secret(id: Uuid, kind: &str) -> Result<(), StorageError> {
    delete_secret_with_attrs(&attrs_for(id, kind)).await
}

async fn delete_secret_with_attrs(attrs: &HashMap<&'static str, String>) -> Result<(), StorageError> {
    let Some(keyring) = open_without_creating_a_keyring().await? else {
        return Ok(());
    };
    keyring.delete(attrs).await.map_err(map_err)?;
    Ok(())
}

async fn open() -> Result<Keyring, StorageError> {
    Keyring::new().await.map_err(open_error)
}

async fn open_without_creating_a_keyring() -> Result<Option<Keyring>, StorageError> {
    if std::path::Path::new("/.flatpak-info").exists() {
        return open().await.map(Some);
    }
    let service = oo7::dbus::Service::new()
        .await
        .map_err(|error| open_error(error.into()))?;
    let collection = service
        .with_alias(oo7::dbus::Service::DEFAULT_COLLECTION)
        .await
        .map_err(|error| open_error(error.into()))?;
    Ok(collection.map(Keyring::DBus))
}

fn open_error(error: oo7::Error) -> StorageError {
    match keyring_failure(&error) {
        crate::KeyringFailure::Other(_) => StorageError::Keyring(crate::KeyringFailure::Unavailable),
        failure => StorageError::Keyring(failure),
    }
}

fn map_err(e: oo7::Error) -> StorageError {
    StorageError::Keyring(keyring_failure(&e))
}

// A Secret Service that is not running surfaces as a D-Bus ServiceUnknown or
// NameHasNoOwner method error; a locked collection as IsLocked; a closed
// unlock prompt as Dismissed.
fn keyring_failure(error: &oo7::Error) -> crate::KeyringFailure {
    use oo7::dbus::{Error as DBus, ServiceError};
    match error {
        oo7::Error::DBus(DBus::Dismissed) => crate::KeyringFailure::UnlockCancelled,
        oo7::Error::DBus(DBus::Service(ServiceError::IsLocked(_))) => crate::KeyringFailure::Locked,
        oo7::Error::DBus(DBus::ZBus(zbus_error)) if names_missing_service(&zbus_error.to_string()) => {
            crate::KeyringFailure::Unavailable
        }
        other => crate::KeyringFailure::Other(other.to_string()),
    }
}

fn names_missing_service(detail: &str) -> bool {
    [
        "ServiceUnknown",
        "NameHasNoOwner",
        "No such file or directory",
        "Connection refused",
    ]
    .iter()
    .any(|marker| detail.contains(marker))
}

fn attrs_for(id: Uuid, kind: &str) -> HashMap<&'static str, String> {
    let mut m = HashMap::new();
    m.insert("xdg:schema", crate::secret_schema().to_string());
    m.insert("connection-id", id.to_string());
    m.insert("kind", kind.to_string());
    m
}

fn attrs_for_hop(
    connection_id: Uuid,
    hop_id: Uuid,
    credential_revision: u64,
    kind: SshHopSecretKind,
) -> HashMap<&'static str, String> {
    let mut attrs = attrs_for(connection_id, kind.attribute_value());
    attrs.insert("hop-id", hop_id.to_string());
    attrs.insert("credential-revision", credential_revision.to_string());
    attrs
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
