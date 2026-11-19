use std::collections::HashMap;

use oo7::Keyring;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use crate::error::StorageError;

const KIND_DB_PASSWORD: &str = "db_password";
const KIND_SSH_PASSWORD: &str = "ssh_password";
const KIND_SSH_PASSPHRASE: &str = "ssh_passphrase";
const KIND_SSH_HOP_PASSWORD: &str = "ssh_hop_password";
const KIND_SSH_HOP_PASSPHRASE: &str = "ssh_hop_passphrase";
const KIND_MCP_TOKEN: &str = "mcp_token";
const KIND_AUDIT_SQL_HASH_KEY: &str = "audit_sql_hash_key";

pub(crate) struct AuditSqlHashKey {
    pub(crate) id: Uuid,
    pub(crate) key: SecretString,
}

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

pub(crate) async fn load_or_create_audit_sql_hash_key() -> Result<AuditSqlHashKey, StorageError> {
    let path = crate::data_path("audit-sql-hash-key")?;
    let _lock = crate::file_access::lock_path(&path).await?;
    let attrs = attrs_for(Uuid::nil(), KIND_AUDIT_SQL_HASH_KEY);
    let (keyring, key) = match open_without_creating_a_keyring().await {
        Ok(Some(keyring)) => match keyring.search_items(&attrs).await {
            Ok(items) => {
                let key = match items.into_iter().next() {
                    Some(item) => {
                        let value = read_secret(&item).await?;
                        Some(parse_audit_sql_hash_key(value.expose_secret())?)
                    }
                    None => None,
                };
                (Some(keyring), key)
            }
            Err(error) if matches!(keyring_failure(&error), crate::KeyringFailure::Unavailable) => (None, None),
            Err(error) => return Err(map_err(error)),
        },
        Ok(None) => (None, None),
        Err(StorageError::Keyring(crate::KeyringFailure::Unavailable)) => (None, None),
        Err(error) => return Err(error),
    };
    if let Some(key) = key {
        return Ok(key);
    }
    if let Some(key) = read_audit_sql_hash_key_file(&path).await? {
        if let Some(keyring) = keyring
            && let Err(error) = store_audit_sql_hash_key(&keyring, &attrs, &key).await
            && !matches!(error, StorageError::Keyring(crate::KeyringFailure::Unavailable))
        {
            return Err(error);
        }
        return Ok(key);
    }
    let key = new_audit_sql_hash_key();
    if let Some(keyring) = keyring {
        match store_audit_sql_hash_key(&keyring, &attrs, &key).await {
            Ok(()) => return Ok(key),
            Err(StorageError::Keyring(crate::KeyringFailure::Unavailable)) => {}
            Err(error) => return Err(error),
        }
    }
    write_audit_sql_hash_key_file(&path, &key).await?;
    Ok(key)
}

async fn read_audit_sql_hash_key_file(path: &std::path::Path) -> Result<Option<AuditSqlHashKey>, StorageError> {
    let Some(bytes) = crate::file_access::read_bounded(path, 128).await? else {
        return Ok(None);
    };
    let value = std::str::from_utf8(&bytes)
        .map_err(|error| StorageError::Schema(format!("audit SQL hash key is not UTF-8: {error}")))?;
    parse_audit_sql_hash_key(value).map(Some)
}

async fn write_audit_sql_hash_key_file(path: &std::path::Path, key: &AuditSqlHashKey) -> Result<(), StorageError> {
    crate::file_access::write_atomically(path, audit_sql_hash_key_value(key).as_bytes()).await
}

async fn store_audit_sql_hash_key(
    keyring: &Keyring,
    attrs: &HashMap<&'static str, String>,
    key: &AuditSqlHashKey,
) -> Result<(), StorageError> {
    keyring
        .create_item(
            "BookiE audit SQL hash key",
            attrs,
            audit_sql_hash_key_value(key).as_bytes(),
            true,
        )
        .await
        .map_err(map_err)
}

fn audit_sql_hash_key_value(key: &AuditSqlHashKey) -> String {
    format!("{}:{}", key.id, key.key.expose_secret())
}

fn parse_audit_sql_hash_key(value: &str) -> Result<AuditSqlHashKey, StorageError> {
    let (id, key) = value
        .split_once(':')
        .ok_or_else(|| StorageError::Schema("audit SQL hash key is malformed".into()))?;
    let id = Uuid::parse_str(id).map_err(|_| StorageError::Schema("audit SQL hash key ID is malformed".into()))?;
    let bytes = hex::decode(key).map_err(|_| StorageError::Schema("audit SQL hash key is malformed".into()))?;
    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|_| StorageError::Schema("audit SQL hash key must be 32 bytes".into()))?;
    Ok(AuditSqlHashKey {
        id,
        key: SecretString::new(hex::encode(key).into()),
    })
}

fn new_audit_sql_hash_key() -> AuditSqlHashKey {
    let mut key = [0; 32];
    key[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    key[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    AuditSqlHashKey {
        id: Uuid::new_v4(),
        key: SecretString::new(hex::encode(key).into()),
    }
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
    if let Some(item) = items.into_iter().next() {
        return read_secret(&item).await.map(Some);
    }
    let legacy = legacy_attrs(attrs);
    let Some(item) = keyring.search_items(&legacy).await.map_err(map_err)?.into_iter().next() else {
        return Ok(None);
    };
    let value = read_secret(&item).await?;
    let label = item.label().await.map_err(map_err)?;
    if let Err(error) = keyring
        .create_item(
            &label,
            attrs,
            secrecy::ExposeSecret::expose_secret(&value).as_bytes(),
            true,
        )
        .await
    {
        tracing::warn!(%error, "credential could not be copied to the current keyring schema");
    }
    Ok(Some(value))
}

async fn read_secret(item: &oo7::Item) -> Result<SecretString, StorageError> {
    let secret = item.secret().await.map_err(map_err)?;
    let s = String::from_utf8(secret.to_vec()).map_err(|e| StorageError::Schema(format!("secret utf8: {e}")))?;
    Ok(SecretString::new(s.into()))
}

fn legacy_attrs(attrs: &HashMap<&'static str, String>) -> HashMap<&'static str, String> {
    let mut legacy = attrs.clone();
    legacy.insert(
        "xdg:schema",
        crate::legacy_state::legacy_secret_schema(crate::secret_schema()),
    );
    legacy
}

async fn delete_secret(id: Uuid, kind: &str) -> Result<(), StorageError> {
    delete_secret_with_attrs(&attrs_for(id, kind)).await
}

async fn delete_secret_with_attrs(attrs: &HashMap<&'static str, String>) -> Result<(), StorageError> {
    let Some(keyring) = open_without_creating_a_keyring().await? else {
        return Ok(());
    };
    keyring.delete(attrs).await.map_err(map_err)?;
    keyring.delete(&legacy_attrs(attrs)).await.map_err(map_err)?;
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
