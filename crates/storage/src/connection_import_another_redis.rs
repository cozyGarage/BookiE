use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::connection_import_dbeaver::{PeerImport, PeerPort, field, port};
use crate::connection_url::{draft_connection, validate_host};
use crate::error::StorageError;
use bookie_core::TlsMode;

pub const MAX_ANOTHER_REDIS_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 1_000;

#[derive(Deserialize)]
#[serde(untagged)]
enum File {
    List(Vec<PeerConnection>),
    Bundle { connections: Vec<PeerConnection> },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeerConnection {
    #[serde(default)]
    name: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    port: Option<PeerPort>,
    #[serde(default)]
    username: String,
    #[serde(default)]
    cluster: bool,
    #[serde(default)]
    connection_read_only: bool,
    #[serde(default)]
    ssh_options: Option<IgnoredAny>,
    #[serde(default)]
    ssl_options: Option<IgnoredAny>,
    #[serde(default)]
    sentinel_options: Option<IgnoredAny>,
}

pub fn parse_another_redis_connections(input: &str) -> Result<PeerImport, StorageError> {
    if input.len() > MAX_ANOTHER_REDIS_BYTES {
        return Err(refused(
            "the file is too large to be an Another Redis Desktop Manager export",
        ));
    }
    let json = decode(input.trim())?;
    let connections = match serde_json::from_str::<File>(&json)
        .map_err(|_| refused("the file is not an Another Redis Desktop Manager connection export"))?
    {
        File::List(list) | File::Bundle { connections: list } => list,
    };
    if connections.is_empty() {
        return Err(refused("the file lists no connections"));
    }
    if connections.len() > MAX_CONNECTIONS {
        return Err(refused("the file lists too many connections"));
    }
    let mut drafts = Vec::new();
    let mut skipped = Vec::new();
    for peer in &connections {
        match draft_from(peer) {
            Some(draft) => drafts.push(draft),
            None => skipped.push(display_name(peer)),
        }
    }
    if drafts.is_empty() {
        return Err(refused(
            "no connection in the file can be imported as a direct Redis connection",
        ));
    }
    drafts.sort_by(|a, b| a.name.cmp(&b.name));
    skipped.sort();
    Ok(PeerImport { drafts, skipped })
}

fn decode(input: &str) -> Result<String, StorageError> {
    if input.starts_with('[') || input.starts_with('{') {
        return Ok(input.to_string());
    }
    let bytes = STANDARD
        .decode(input)
        .map_err(|_| refused("the file is not an Another Redis Desktop Manager connection export"))?;
    String::from_utf8(bytes).map_err(|_| refused("the file is not an Another Redis Desktop Manager connection export"))
}

fn draft_from(peer: &PeerConnection) -> Option<crate::connections::SavedConnection> {
    if peer.ssh_options.is_some() || peer.sentinel_options.is_some() || peer.cluster {
        return None;
    }
    let host = validate_host(&peer.host).ok()?;
    let port = port(peer.port.as_ref(), "redis")?;
    let username = field(&peer.username)?;
    let name = if peer.name.trim().is_empty() {
        host.clone()
    } else {
        field(peer.name.trim())?
    };
    let mut draft = draft_connection("redis", name, host, port, String::new(), username);
    draft.read_only = peer.connection_read_only;
    if peer.ssl_options.is_some() {
        draft.use_tls = true;
        draft.tls_mode = Some(TlsMode::VerifyFull);
    }
    Some(draft)
}

fn display_name(peer: &PeerConnection) -> String {
    let name = if peer.name.trim().is_empty() {
        peer.host.trim()
    } else {
        peer.name.trim()
    };
    name.chars().filter(|c| !c.is_control()).take(256).collect()
}

fn refused(message: &str) -> StorageError {
    StorageError::Schema(message.to_string())
}

#[cfg(test)]
#[path = "connection_import_another_redis_tests.rs"]
mod tests;
