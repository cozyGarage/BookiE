use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::StorageError;
use crate::file_access::{lock_path, read_bounded, write_atomically};
use tablepro_core::{AuthMode, Environment, TlsMode};

const CURRENT_VERSION: u32 = 2;
const MAX_CONNECTIONS_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 1_000;
const MAX_CONNECTION_VALUE_BYTES: usize = 64 * 1024;

#[cfg(test)]
#[path = "connections_migration_tests.rs"]
mod migration_tests;

#[path = "connections_copy.rs"]
mod copy;
pub use copy::duplicate_connection;
#[cfg(test)]
use copy::duplicate_from;

#[cfg(test)]
#[path = "connections_update_tests.rs"]
mod update_tests;

#[cfg(test)]
#[path = "connections_socket_tests.rs"]
mod socket_tests;
/// Hops one saved SSH chain may declare. `jump` deserializes
/// recursively, so without a cap here the only thing standing between
/// an edited connections file and a deep recursive parse is
/// serde_json's default 128-level nesting limit.
pub(crate) const MAX_SSH_HOPS: usize = 8;

/// Every JSON key `SavedConnection` owns. A key outside this list came
/// from a newer TablePro (or a hand edit) and is carried through a
/// rewrite untouched, so downgrading does not silently strip a field
/// the newer version depends on. Keys inside the list are always taken
/// from the freshly serialized record — a cleared `Option` must not be
/// resurrected from the copy on disk.
pub(crate) const KNOWN_CONNECTION_FIELDS: &[&str] = &[
    "id",
    "name",
    "driver_id",
    "host",
    "port",
    "socket_dir",
    "socket_path",
    "database",
    "username",
    "use_tls",
    "tls_mode",
    "tls_root_cert",
    "tls_client_cert",
    "tls_client_key",
    "read_only",
    "auth_mode",
    "environment",
    "ssh",
    "last_opened_at",
    "auth_source",
];

const KNOWN_FILE_FIELDS: &[&str] = &["version", "connections"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedConnection {
    pub id: Uuid,
    pub name: String,
    pub driver_id: String,
    pub host: String,
    pub port: u16,
    /// PostgreSQL local Unix-domain socket directory. Omitted for network
    /// connections and legacy records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<PathBuf>,
    pub database: String,
    pub username: String,
    /// Legacy boolean TLS flag. Prefer `tls_mode`. Kept for round-trip of
    /// files written before Stage 1; new writes always set `tls_mode`.
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_mode: Option<TlsMode>,
    /// Certificate authority used to verify the server, for engines whose
    /// certificate is not issued by a CA in the system trust store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_root_cert: Option<PathBuf>,
    /// Client certificate and key paths for engines with explicit mTLS support.
    /// The private key material itself is never copied into the saved record.
    #[serde(default)]
    pub tls_client_cert: Option<PathBuf>,
    #[serde(default)]
    pub tls_client_key: Option<PathBuf>,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub auth_mode: AuthMode,
    #[serde(default)]
    pub environment: Environment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SavedSshConfig>,
    /// Last successful open of this connection. Drives the welcome
    /// view's recency-first sort. `None` for connections saved before
    /// this field shipped (legacy files just deserialize into None);
    /// they sort after every connection that has been opened at least
    /// once and fall back to alphabetical against each other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_opened_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_timeout_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_timeout_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_source: Option<String>,
}

impl SavedConnection {
    pub fn effective_tls_mode(&self) -> TlsMode {
        self.tls_mode.unwrap_or(if self.use_tls {
            TlsMode::VerifyFull
        } else {
            TlsMode::Disabled
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSshConfig {
    #[serde(default = "Uuid::new_v4")]
    pub hop_id: Uuid,
    #[serde(default)]
    pub credential_revision: u64,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SavedSshAuth,
    /// Next hop toward the database (ProxyJump-style chain).
    /// `None` means this is the last jump before the DB host.
    /// Omitted in legacy files via `#[serde(default)]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump: Option<Box<SavedSshConfig>>,
    #[serde(default, skip_serializing_if = "SshClient::is_builtin")]
    pub client: SshClient,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub agent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_identity: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshClient {
    #[default]
    Builtin,
    OpenSsh,
}

impl SshClient {
    fn is_builtin(&self) -> bool {
        *self == Self::Builtin
    }
}

impl SavedSshConfig {
    /// Flatten nested `jump` links into hop order: first bastion first.
    pub fn flatten_hops(&self) -> Vec<&SavedSshConfig> {
        let mut out = Vec::new();
        let mut cur = Some(self);
        while let Some(hop) = cur {
            out.push(hop);
            cur = hop.jump.as_deref();
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SavedSshAuth {
    Password,
    PrivateKey {
        path: PathBuf,
        #[serde(default)]
        has_passphrase: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct ConnectionsFile {
    version: u32,
    connections: Vec<SavedConnection>,
}

pub async fn load_connections() -> Result<Vec<SavedConnection>, StorageError> {
    load_from(&connections_path()?).await
}

pub async fn save_connections(connections: &[SavedConnection]) -> Result<(), StorageError> {
    upsert_to(&connections_path()?, connections).await
}

pub async fn delete_connection(id: Uuid) -> Result<(), StorageError> {
    delete_from(&connections_path()?, id).await
}

/// Apply an import plan as one rewrite. Taking the lock once means the
/// whole plan lands or none of it does, rather than N upserts racing
/// another process between them.
pub async fn apply_import(plan: &crate::connection_bundle::ImportPlan) -> Result<(), StorageError> {
    apply_import_to(&connections_path()?, plan).await
}

pub(crate) async fn apply_import_to(
    path: &Path,
    plan: &crate::connection_bundle::ImportPlan,
) -> Result<(), StorageError> {
    let _guard = lock_path(path).await?;
    let mut existing = load_from(path).await?;
    for item in &plan.items {
        existing.retain(|saved| saved.id != item.connection.id);
        existing.push(item.connection.clone());
    }
    save_to_unlocked(path, &existing).await
}

/// Undo one item of an import. A record that existed before the import
/// is restored from its snapshot; one the import created is removed.
/// There is no path here that removes a record the bundle did not
/// create, because import is additive.
pub async fn restore_connection(previous: Option<&SavedConnection>, id: Uuid) -> Result<(), StorageError> {
    match previous {
        Some(connection) => save_connections(std::slice::from_ref(connection)).await,
        None => delete_connection(id).await,
    }
}

async fn delete_from(path: &Path, id: Uuid) -> Result<(), StorageError> {
    let _guard = lock_path(path).await?;
    let mut existing = load_from(path).await?;
    existing.retain(|connection| connection.id != id);
    save_to_unlocked(path, &existing).await
}

/// Stamp `last_opened_at = now()` on the matching connection. Called
/// once per successful open so the welcome view can sort recency-first.
/// No-op when `id` isn't in the file (e.g. an unsaved connection
/// opened from the dialog without ticking "Save"); the missing-id case
/// is silent because there is nothing to update.
pub async fn touch_last_opened(id: Uuid) -> Result<(), StorageError> {
    update_connection(id, |connection| connection.last_opened_at = Some(Utc::now()))
        .await
        .map(drop)
}

pub async fn update_connection(
    id: Uuid,
    change: impl FnOnce(&mut SavedConnection),
) -> Result<Option<SavedConnection>, StorageError> {
    update_connection_in(&connections_path()?, id, change).await
}

pub(crate) async fn update_connection_in(
    path: &Path,
    id: Uuid,
    change: impl FnOnce(&mut SavedConnection),
) -> Result<Option<SavedConnection>, StorageError> {
    let _guard = lock_path(path).await?;
    let mut existing = load_from(path).await?;
    let Some(connection) = existing.iter_mut().find(|connection| connection.id == id) else {
        return Ok(None);
    };
    change(connection);
    let updated = connection.clone();
    save_to_unlocked(path, &existing).await?;
    Ok(Some(updated))
}

pub(crate) async fn load_from(path: &Path) -> Result<Vec<SavedConnection>, StorageError> {
    let Some(bytes) = read_bounded(path, MAX_CONNECTIONS_FILE_BYTES).await? else {
        return Ok(Vec::new());
    };
    let file: ConnectionsFile = serde_json::from_slice(&bytes)?;
    validate_file(&file)?;
    Ok(file.connections)
}

#[cfg(test)]
pub(crate) async fn save_to(path: &Path, connections: &[SavedConnection]) -> Result<(), StorageError> {
    let _guard = lock_path(path).await?;
    save_to_unlocked(path, connections).await
}

async fn upsert_to(path: &Path, connections: &[SavedConnection]) -> Result<(), StorageError> {
    validate_connections(connections)?;
    let _guard = lock_path(path).await?;
    let mut existing = load_from(path).await?;
    for connection in connections {
        existing.retain(|saved| saved.id != connection.id);
        existing.push(connection.clone());
    }
    save_to_unlocked(path, &existing).await
}

async fn save_to_unlocked(path: &Path, connections: &[SavedConnection]) -> Result<(), StorageError> {
    validate_connections(connections)?;
    let stored = read_raw_file(path).await?;
    let file = ConnectionsFile {
        version: CURRENT_VERSION,
        connections: connections.to_vec(),
    };
    let mut document = serde_json::to_value(&file)?;
    if let Some(stored) = stored.as_ref() {
        carry_unknown_fields(&mut document, stored);
    }
    let json = serde_json::to_vec_pretty(&document)?;
    if json.len() > MAX_CONNECTIONS_FILE_BYTES {
        return Err(StorageError::TooLarge {
            got: json.len(),
            limit: MAX_CONNECTIONS_FILE_BYTES,
        });
    }
    write_atomically(path, &json).await
}

async fn read_raw_file(path: &Path) -> Result<Option<serde_json::Map<String, serde_json::Value>>, StorageError> {
    let Some(bytes) = read_bounded(path, MAX_CONNECTIONS_FILE_BYTES).await? else {
        return Ok(None);
    };
    let stored: serde_json::Value = serde_json::from_slice(&bytes)?;
    let object = stored
        .as_object()
        .cloned()
        .ok_or_else(|| StorageError::Schema("connections.json must contain an object".into()))?;
    let file: ConnectionsFile = serde_json::from_value(serde_json::Value::Object(object.clone()))?;
    validate_file(&file)?;
    Ok(Some(object))
}

fn validate_file(file: &ConnectionsFile) -> Result<(), StorageError> {
    if !(1..=CURRENT_VERSION).contains(&file.version) {
        return Err(StorageError::Schema(format!(
            "connections.json version {} not supported (expected {})",
            file.version, CURRENT_VERSION,
        )));
    }
    validate_connections(&file.connections)
}

fn validate_connections(connections: &[SavedConnection]) -> Result<(), StorageError> {
    if connections.len() > MAX_CONNECTIONS {
        return Err(StorageError::Schema(format!(
            "connections are limited to {MAX_CONNECTIONS} entries"
        )));
    }
    for connection in connections {
        validate_connection(connection)?;
    }
    Ok(())
}

fn validate_connection(connection: &SavedConnection) -> Result<(), StorageError> {
    if connection.tls_client_cert.is_some() != connection.tls_client_key.is_some() {
        return Err(StorageError::Schema(
            "TLS client authentication requires both a certificate and a private key".into(),
        ));
    }
    for value in [
        connection.name.as_str(),
        connection.driver_id.as_str(),
        connection.host.as_str(),
        connection.database.as_str(),
        connection.username.as_str(),
    ] {
        validate_value_size(value.len())?;
    }
    for path in [
        connection.socket_dir.as_deref(),
        connection.socket_path.as_deref(),
        connection.tls_root_cert.as_deref(),
        connection.tls_client_cert.as_deref(),
        connection.tls_client_key.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_value_size(path.as_os_str().as_encoded_bytes().len())?;
    }
    let mut ssh = connection.ssh.as_ref();
    let mut hops = 0usize;
    while let Some(hop) = ssh {
        hops += 1;
        if hops > MAX_SSH_HOPS {
            return Err(StorageError::Schema(format!(
                "an SSH chain is limited to {MAX_SSH_HOPS} hops"
            )));
        }
        validate_value_size(hop.host.len())?;
        validate_value_size(hop.username.len())?;
        if let SavedSshAuth::PrivateKey { path, .. } = &hop.auth {
            validate_value_size(path.as_os_str().as_encoded_bytes().len())?;
        }
        ssh = hop.jump.as_deref();
    }
    Ok(())
}

fn validate_value_size(size: usize) -> Result<(), StorageError> {
    if size > MAX_CONNECTION_VALUE_BYTES {
        return Err(StorageError::TooLarge {
            got: size,
            limit: MAX_CONNECTION_VALUE_BYTES,
        });
    }
    Ok(())
}

fn carry_unknown_fields(document: &mut serde_json::Value, stored: &serde_json::Map<String, serde_json::Value>) {
    let Some(fresh) = document.as_object_mut() else {
        return;
    };
    for (key, value) in stored {
        if KNOWN_FILE_FIELDS.contains(&key.as_str()) {
            continue;
        }
        fresh.entry(key.clone()).or_insert_with(|| value.clone());
    }
    let stored_records = stored.get("connections").and_then(|value| value.as_array());
    let Some(stored_records) = stored_records else {
        return;
    };
    let Some(fresh_records) = fresh.get_mut("connections").and_then(|value| value.as_array_mut()) else {
        return;
    };
    for record in fresh_records {
        let Some(id) = record.get("id").and_then(|value| value.as_str()).map(str::to_owned) else {
            continue;
        };
        let Some(previous) = stored_records
            .iter()
            .filter_map(|candidate| candidate.as_object())
            .find(|candidate| candidate.get("id").and_then(|value| value.as_str()) == Some(id.as_str()))
        else {
            continue;
        };
        let Some(target) = record.as_object_mut() else {
            continue;
        };
        for (key, value) in previous {
            if KNOWN_CONNECTION_FIELDS.contains(&key.as_str()) {
                continue;
            }
            target.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
}

fn connections_path() -> Result<PathBuf, StorageError> {
    crate::config_path("connections.json")
}

#[cfg(test)]
#[path = "connections_tests.rs"]
mod tests;
