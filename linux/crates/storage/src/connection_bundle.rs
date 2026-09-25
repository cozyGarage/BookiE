use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use secrecy::{ExposeSecret, SecretString};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use tablepro_core::{AuthMode, Environment, TlsMode};

use crate::connection_bundle_crypto::{
    CipherHeader, EnvelopeHeader, KdfHeader, SealedBundle, check_kdf_parameters, open, seal,
};
use crate::connection_organization::ConnectionOrganization;
use crate::connections::{SavedConnection, SavedSshConfig};

pub const BUNDLE_FORMAT: &str = "tablepro.connection-bundle";
pub const BUNDLE_VERSION: u32 = 1;

pub const MAX_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_BUNDLE_CONNECTIONS: usize = 1_000;
/// Hops a bundled SSH chain may declare. Deserialising `jump` is
/// recursive, so without a cap the only defence is serde_json's default
/// 128-level nesting limit.
pub const MAX_SSH_HOPS: usize = 8;

const KIND_PLAINTEXT: &str = "plaintext";
const KIND_ENCRYPTED: &str = "encrypted";

/// Fields of a saved connection that a bundle carries.
pub const BUNDLE_INCLUDED_FIELDS: &[&str] = &[
    "id",
    "name",
    "driver_id",
    "host",
    "port",
    "socket_dir",
    "database",
    "username",
    "tls_mode",
    "tls_root_cert",
    "read_only",
    "auth_mode",
    "environment",
    "ssh",
];

/// Fields a bundle deliberately drops. `use_tls` is the legacy boolean
/// that `tls_mode` replaced, so exporting it would let a bundle claim
/// `use_tls: true, tls_mode: disabled`. `last_opened_at` is local
/// telemetry that would tell a recipient when each production database
/// was last touched.
pub const BUNDLE_EXCLUDED_FIELDS: &[&str] = &["use_tls", "last_opened_at"];

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("this file is not a BookiE connection bundle")]
    NotABundle,
    #[error("this bundle was written by a newer version of BookiE (format version {0})")]
    UnsupportedVersion(u32),
    #[error("the bundle is not valid JSON")]
    Malformed,
    #[error("the bundle field {0} is missing or malformed")]
    Field(&'static str),
    #[error("a bundle without a passphrase cannot carry saved passwords")]
    PlaintextSecrets,
    #[error("the bundle is {got} bytes, over the {limit} byte limit")]
    TooLarge { got: usize, limit: usize },
    #[error("the bundle holds {got} connections, over the limit of {limit}")]
    TooManyConnections { got: usize, limit: usize },
    #[error("an SSH chain in the bundle is {got} hops deep, over the limit of {limit}")]
    SshChainTooDeep { got: usize, limit: usize },
    #[error("the bundle lists the same connection twice")]
    DuplicateConnectionId,
    #[error("the key-derivation settings in this bundle are outside the accepted range")]
    KdfParameters,
    #[error("The passphrase is wrong, or the file was changed after it was exported.")]
    DecryptionFailed,
    #[error("the bundle could not be encrypted")]
    EncryptionFailed,
    #[error("a passphrase is required")]
    EmptyPassphrase,
    #[error("the saved password for connection {0} could not be read")]
    SecretUnavailable(Uuid),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleConnection {
    pub id: Uuid,
    pub name: String,
    pub driver_id: String,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_dir: Option<PathBuf>,
    pub database: String,
    pub username: String,
    pub tls_mode: TlsMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_root_cert: Option<PathBuf>,
    pub read_only: bool,
    pub auth_mode: AuthMode,
    pub environment: Environment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SavedSshConfig>,
}

impl BundleConnection {
    pub fn from_saved(saved: &SavedConnection) -> Self {
        Self {
            id: saved.id,
            name: saved.name.clone(),
            driver_id: saved.driver_id.clone(),
            host: saved.host.clone(),
            port: saved.port,
            socket_dir: saved.socket_dir.clone(),
            database: saved.database.clone(),
            username: saved.username.clone(),
            tls_mode: saved.effective_tls_mode(),
            tls_root_cert: saved.tls_root_cert.clone(),
            read_only: saved.read_only,
            auth_mode: saved.auth_mode,
            environment: saved.environment,
            ssh: saved.ssh.clone(),
        }
    }

    pub fn to_saved(&self) -> SavedConnection {
        SavedConnection {
            id: self.id,
            name: self.name.clone(),
            driver_id: self.driver_id.clone(),
            host: self.host.clone(),
            port: self.port,
            socket_dir: self.socket_dir.clone(),
            database: self.database.clone(),
            username: self.username.clone(),
            use_tls: self.tls_mode.encrypts(),
            tls_mode: Some(self.tls_mode),
            tls_root_cert: self.tls_root_cert.clone(),
            read_only: self.read_only,
            auth_mode: self.auth_mode,
            environment: self.environment,
            ssh: self.ssh.clone(),
            last_opened_at: None,
            connect_timeout_secs: None,
            query_timeout_secs: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleOrganization {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Credentials for one connection. Never derives `Debug` or
/// `Serialize`: both are written by hand so a stray `{:?}` or a future
/// field cannot put a password in a log line or on disk unencrypted.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleSecrets {
    pub connection_id: Uuid,
    #[serde(default)]
    db_password: Option<SecretString>,
    #[serde(default)]
    ssh_password: Option<SecretString>,
    #[serde(default)]
    ssh_passphrase: Option<SecretString>,
}

impl BundleSecrets {
    pub fn new(
        connection_id: Uuid,
        db_password: Option<SecretString>,
        ssh_password: Option<SecretString>,
        ssh_passphrase: Option<SecretString>,
    ) -> Self {
        Self {
            connection_id,
            db_password,
            ssh_password,
            ssh_passphrase,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.db_password.is_none() && self.ssh_password.is_none() && self.ssh_passphrase.is_none()
    }

    pub fn db_password(&self) -> Option<&SecretString> {
        self.db_password.as_ref()
    }

    pub fn ssh_password(&self) -> Option<&SecretString> {
        self.ssh_password.as_ref()
    }

    pub fn ssh_passphrase(&self) -> Option<&SecretString> {
        self.ssh_passphrase.as_ref()
    }
}

impl std::fmt::Debug for BundleSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BundleSecrets")
            .field("connection_id", &self.connection_id)
            .finish_non_exhaustive()
    }
}

impl Serialize for BundleSecrets {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("BundleSecrets", 4)?;
        state.serialize_field("connection_id", &self.connection_id)?;
        state.serialize_field("db_password", &self.db_password.as_ref().map(|v| v.expose_secret()))?;
        state.serialize_field("ssh_password", &self.ssh_password.as_ref().map(|v| v.expose_secret()))?;
        state.serialize_field(
            "ssh_passphrase",
            &self.ssh_passphrase.as_ref().map(|v| v.expose_secret()),
        )?;
        state.end()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleBody {
    pub connections: Vec<BundleConnection>,
    #[serde(default)]
    pub organization: BTreeMap<Uuid, BundleOrganization>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<BundleSecrets>,
}

/// `#[serde(deny_unknown_fields)]` does not reach through
/// `#[serde(tag = ...)]` (serde#1358), so the payload keeps an explicit
/// `kind` string that `parse` matches by hand instead of an internally
/// tagged enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundlePayload {
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<BundleBody>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kdf: Option<KdfHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cipher: Option<CipherHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ciphertext: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleDocument {
    format: String,
    version: u32,
    producer: String,
    exported_at: String,
    payload: BundlePayload,
}

/// Everything a bundle export needs that is not a policy decision.
pub struct BundleExport {
    pub producer: String,
    pub exported_at: String,
    pub connections: Vec<SavedConnection>,
    pub organization: Vec<(Uuid, ConnectionOrganization)>,
    pub secrets: Vec<BundleSecrets>,
}

impl BundleExport {
    fn body(&self, include_secrets: bool) -> BundleBody {
        BundleBody {
            connections: self.connections.iter().map(BundleConnection::from_saved).collect(),
            organization: self
                .organization
                .iter()
                .map(|(id, entry)| (*id, bundle_organization(entry)))
                .collect(),
            secrets: if include_secrets {
                self.secrets.clone()
            } else {
                Vec::new()
            },
        }
    }

    fn envelope(&self) -> EnvelopeHeader {
        EnvelopeHeader {
            version: BUNDLE_VERSION,
            producer: self.producer.clone(),
            exported_at: self.exported_at.clone(),
        }
    }
}

pub fn bundle_organization(entry: &ConnectionOrganization) -> BundleOrganization {
    BundleOrganization {
        group: entry.group.clone(),
        tags: entry.tags.clone(),
        color: entry.color.clone(),
    }
}

/// A bundle nobody has to type a passphrase for. It carries no
/// credentials at all: `secrets` is not populated on this path, and
/// `parse` rejects a plaintext payload that arrives with any.
pub fn export_plaintext(export: &BundleExport) -> Result<Vec<u8>, BundleError> {
    let payload = BundlePayload {
        kind: KIND_PLAINTEXT.to_owned(),
        body: Some(export.body(false)),
        kdf: None,
        cipher: None,
        ciphertext: None,
    };
    finish(export, payload)
}

pub fn export_encrypted(export: &BundleExport, passphrase: &str) -> Result<Vec<u8>, BundleError> {
    let body = export.body(true);
    let mut plaintext = Zeroizing::new(Vec::new());
    serde_json::to_writer(&mut *plaintext, &body).map_err(|_| BundleError::EncryptionFailed)?;
    let sealed = seal(&plaintext, passphrase, &export.envelope())?;
    drop(plaintext);
    let payload = BundlePayload {
        kind: KIND_ENCRYPTED.to_owned(),
        body: None,
        kdf: Some(sealed.kdf),
        cipher: Some(sealed.cipher),
        ciphertext: Some(BASE64.encode(&sealed.ciphertext)),
    };
    finish(export, payload)
}

fn finish(export: &BundleExport, payload: BundlePayload) -> Result<Vec<u8>, BundleError> {
    let document = BundleDocument {
        format: BUNDLE_FORMAT.to_owned(),
        version: BUNDLE_VERSION,
        producer: export.producer.clone(),
        exported_at: export.exported_at.clone(),
        payload,
    };
    let json = serde_json::to_vec_pretty(&document).map_err(|_| BundleError::Malformed)?;
    if json.len() > MAX_BUNDLE_BYTES {
        return Err(BundleError::TooLarge {
            got: json.len(),
            limit: MAX_BUNDLE_BYTES,
        });
    }
    Ok(json)
}

/// A parsed bundle whose body is either already available or still
/// waiting on a passphrase.
pub enum ParsedBundle {
    Plaintext(BundleBody),
    Encrypted(EncryptedBundle),
}

pub struct EncryptedBundle {
    envelope: EnvelopeHeader,
    sealed: SealedBundle,
}

impl std::fmt::Debug for EncryptedBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedBundle").finish_non_exhaustive()
    }
}

pub fn parse_bundle(bytes: &[u8]) -> Result<ParsedBundle, BundleError> {
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(BundleError::TooLarge {
            got: bytes.len(),
            limit: MAX_BUNDLE_BYTES,
        });
    }
    let document: BundleDocument = serde_json::from_slice(bytes).map_err(|_| BundleError::Malformed)?;
    if document.format != BUNDLE_FORMAT {
        return Err(BundleError::NotABundle);
    }
    if document.version != BUNDLE_VERSION {
        return Err(BundleError::UnsupportedVersion(document.version));
    }
    let envelope = EnvelopeHeader {
        version: document.version,
        producer: document.producer,
        exported_at: document.exported_at,
    };
    match document.payload.kind.as_str() {
        KIND_PLAINTEXT => parse_plaintext(document.payload),
        KIND_ENCRYPTED => parse_encrypted(envelope, document.payload),
        _ => Err(BundleError::Field("payload.kind")),
    }
}

fn parse_plaintext(payload: BundlePayload) -> Result<ParsedBundle, BundleError> {
    if payload.kdf.is_some() || payload.cipher.is_some() || payload.ciphertext.is_some() {
        return Err(BundleError::Field("payload.body"));
    }
    let body = payload.body.ok_or(BundleError::Field("payload.body"))?;
    if !body.secrets.is_empty() {
        return Err(BundleError::PlaintextSecrets);
    }
    check_body_bounds(&body)?;
    Ok(ParsedBundle::Plaintext(body))
}

fn parse_encrypted(envelope: EnvelopeHeader, payload: BundlePayload) -> Result<ParsedBundle, BundleError> {
    if payload.body.is_some() {
        return Err(BundleError::Field("payload.body"));
    }
    let kdf = payload.kdf.ok_or(BundleError::Field("payload.kdf"))?;
    let cipher = payload.cipher.ok_or(BundleError::Field("payload.cipher"))?;
    let encoded = payload.ciphertext.ok_or(BundleError::Field("payload.ciphertext"))?;
    check_kdf_parameters(&kdf)?;
    let ciphertext = BASE64
        .decode(encoded)
        .map_err(|_| BundleError::Field("payload.ciphertext"))?;
    if ciphertext.len() > MAX_BUNDLE_BYTES {
        return Err(BundleError::TooLarge {
            got: ciphertext.len(),
            limit: MAX_BUNDLE_BYTES,
        });
    }
    Ok(ParsedBundle::Encrypted(EncryptedBundle {
        envelope,
        sealed: SealedBundle {
            kdf,
            cipher,
            ciphertext,
        },
    }))
}

impl EncryptedBundle {
    pub fn unlock(&self, passphrase: &str) -> Result<BundleBody, BundleError> {
        let plaintext = open(&self.sealed, passphrase, &self.envelope)?;
        let body: BundleBody = serde_json::from_slice(&plaintext).map_err(|_| BundleError::Malformed)?;
        drop(plaintext);
        check_body_bounds(&body)?;
        Ok(body)
    }
}

fn check_body_bounds(body: &BundleBody) -> Result<(), BundleError> {
    if body.connections.len() > MAX_BUNDLE_CONNECTIONS {
        return Err(BundleError::TooManyConnections {
            got: body.connections.len(),
            limit: MAX_BUNDLE_CONNECTIONS,
        });
    }
    for connection in &body.connections {
        check_ssh_depth(connection.ssh.as_ref())?;
    }
    Ok(())
}

pub fn check_ssh_depth(ssh: Option<&SavedSshConfig>) -> Result<(), BundleError> {
    let Some(root) = ssh else {
        return Ok(());
    };
    let mut hops = 0usize;
    let mut cursor = Some(root);
    while let Some(hop) = cursor {
        hops += 1;
        if hops > MAX_SSH_HOPS {
            return Err(BundleError::SshChainTooDeep {
                got: hops,
                limit: MAX_SSH_HOPS,
            });
        }
        cursor = hop.jump.as_deref();
    }
    Ok(())
}

/// Read every credential a bundle may carry for these connections.
/// Fails closed: a keyring that cannot answer for one connection aborts
/// the whole export, because a bundle quietly missing passwords the user
/// believed were in it is worse than no bundle.
pub async fn collect_bundle_secrets(ids: &[Uuid]) -> Result<Vec<BundleSecrets>, BundleError> {
    let mut collected = Vec::with_capacity(ids.len());
    for id in ids {
        let secrets = BundleSecrets::new(
            *id,
            crate::secrets::load_password(*id)
                .await
                .map_err(|_| BundleError::SecretUnavailable(*id))?,
            crate::secrets::load_ssh_password(*id)
                .await
                .map_err(|_| BundleError::SecretUnavailable(*id))?,
            crate::secrets::load_ssh_passphrase(*id)
                .await
                .map_err(|_| BundleError::SecretUnavailable(*id))?,
        );
        if secrets.is_empty() {
            continue;
        }
        collected.push(secrets);
    }
    Ok(collected)
}

/// Write an imported connection's credentials under `target`. An
/// existing local secret is kept unless `replace_existing` is set, so an
/// import cannot silently swap a password the user still relies on.
pub async fn store_bundle_secrets(
    target: Uuid,
    secrets: &BundleSecrets,
    label: &str,
    replace_existing: bool,
) -> Result<(), crate::error::StorageError> {
    if let Some(value) = secrets.db_password()
        && should_write(crate::secrets::load_password(target).await?, replace_existing)
    {
        crate::secrets::store_password(target, value.expose_secret(), label).await?;
    }
    if let Some(value) = secrets.ssh_password()
        && should_write(crate::secrets::load_ssh_password(target).await?, replace_existing)
    {
        crate::secrets::store_ssh_password(target, value.expose_secret(), label).await?;
    }
    if let Some(value) = secrets.ssh_passphrase()
        && should_write(crate::secrets::load_ssh_passphrase(target).await?, replace_existing)
    {
        crate::secrets::store_ssh_passphrase(target, value.expose_secret(), label).await?;
    }
    Ok(())
}

pub(crate) fn should_write(existing: Option<SecretString>, replace_existing: bool) -> bool {
    existing.is_none() || replace_existing
}

/// Best-effort removal of the credentials an import wrote for a
/// connection it created. Used only on the rollback path for a `New`
/// item, never for one that existed before the import.
pub async fn forget_imported_secrets(target: Uuid) {
    for outcome in [
        crate::secrets::delete_password(target).await,
        crate::secrets::delete_ssh_password(target).await,
        crate::secrets::delete_ssh_passphrase(target).await,
    ] {
        if let Err(error) = outcome {
            tracing::warn!(error = %error, "removing a partially imported credential failed");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportDisposition {
    /// The id is free. Keep it, so a restored backup lines up with a
    /// restored keyring, organisation file, favourites and history.
    New,
    /// The id is taken by the same endpoint. Update the settings.
    UpdateInPlace,
    /// The id is taken by a different server. The audit journal,
    /// history rows and policy overrides already bind that id, so the
    /// incoming record gets a fresh one instead of repointing it.
    Remapped,
}

#[derive(Debug, Clone)]
pub struct ImportItem {
    pub disposition: ImportDisposition,
    pub source_id: Uuid,
    pub connection: SavedConnection,
    pub previous: Option<SavedConnection>,
    pub organization: Option<BundleOrganization>,
}

#[derive(Debug, Clone, Default)]
pub struct ImportPlan {
    pub items: Vec<ImportItem>,
}

pub fn plan_import(local: &[SavedConnection], body: &BundleBody) -> Result<ImportPlan, BundleError> {
    check_body_bounds(body)?;
    let mut seen: HashSet<Uuid> = HashSet::new();
    let mut items = Vec::with_capacity(body.connections.len());
    for incoming in &body.connections {
        if !seen.insert(incoming.id) {
            return Err(BundleError::DuplicateConnectionId);
        }
        items.push(plan_one(local, incoming, body.organization.get(&incoming.id)));
    }
    Ok(ImportPlan { items })
}

fn plan_one(
    local: &[SavedConnection],
    incoming: &BundleConnection,
    organization: Option<&BundleOrganization>,
) -> ImportItem {
    let organization = organization.cloned();
    let candidate = incoming.to_saved();
    let Some(existing) = local.iter().find(|saved| saved.id == incoming.id) else {
        return ImportItem {
            disposition: ImportDisposition::New,
            source_id: incoming.id,
            connection: candidate,
            previous: None,
            organization,
        };
    };
    if endpoint_key(existing) != endpoint_key(&candidate) {
        let mut remapped = candidate;
        remapped.id = Uuid::new_v4();
        return ImportItem {
            disposition: ImportDisposition::Remapped,
            source_id: incoming.id,
            connection: remapped,
            previous: None,
            organization,
        };
    }
    ImportItem {
        disposition: ImportDisposition::UpdateInPlace,
        source_id: incoming.id,
        connection: merge_without_downgrade(existing, candidate),
        previous: Some(existing.clone()),
        organization,
    }
}

/// Settings a bundle may change, with the safety ones ratcheted: an
/// import can tighten a local connection and never loosen it.
fn merge_without_downgrade(local: &SavedConnection, incoming: SavedConnection) -> SavedConnection {
    let tls_mode = stricter_tls(local.effective_tls_mode(), incoming.effective_tls_mode());
    SavedConnection {
        read_only: local.read_only || incoming.read_only,
        environment: higher_environment(local.environment, incoming.environment),
        use_tls: tls_mode.encrypts(),
        tls_mode: Some(tls_mode),
        last_opened_at: local.last_opened_at,
        ..incoming
    }
}

fn tls_rank(mode: TlsMode) -> u8 {
    match mode {
        TlsMode::Disabled => 0,
        TlsMode::Prefer => 1,
        TlsMode::Require => 2,
        TlsMode::VerifyCa => 3,
        TlsMode::VerifyFull => 4,
    }
}

fn stricter_tls(left: TlsMode, right: TlsMode) -> TlsMode {
    if tls_rank(right) > tls_rank(left) { right } else { left }
}

fn environment_rank(environment: Environment) -> u8 {
    match environment {
        Environment::Local => 0,
        Environment::Dev => 1,
        Environment::Staging => 2,
        Environment::Prod => 3,
    }
}

fn higher_environment(left: Environment, right: Environment) -> Environment {
    if environment_rank(right) > environment_rank(left) {
        right
    } else {
        left
    }
}

type EndpointKey = (String, String, u16, Option<PathBuf>, String, String, Vec<SshHopKey>);
type SshHopKey = (String, u16, String);

/// What makes two records the same server reached the same way. Name,
/// TLS mode, read-only, environment and auth mode are settings, not
/// identity, so a bundle may change them in place.
fn endpoint_key(saved: &SavedConnection) -> EndpointKey {
    let hops = saved
        .ssh
        .as_ref()
        .map(|root| {
            root.flatten_hops()
                .into_iter()
                .map(|hop| (hop.host.to_lowercase(), hop.port, hop.username.clone()))
                .collect()
        })
        .unwrap_or_default();
    (
        saved.driver_id.clone(),
        saved.host.to_lowercase(),
        saved.port,
        saved.socket_dir.clone(),
        saved.database.clone(),
        saved.username.clone(),
        hops,
    )
}

#[cfg(test)]
#[path = "connection_bundle_tests.rs"]
mod tests;
