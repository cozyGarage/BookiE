use serde::Deserialize;

use crate::connection_import_peer::{PeerImport, PeerPort, field, port, refused, skipped_label};
use crate::connection_url::{draft_connection, validate_host};
use crate::connections::SavedConnection;
use crate::error::StorageError;
use bookie_core::TlsMode;

pub const MAX_RESP_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 1_000;

#[derive(Deserialize)]
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
    ssl: bool,
    #[serde(default)]
    ssh_host: String,
    #[serde(default)]
    namespace_separator: Option<String>,
    #[serde(default)]
    keys_pattern: Option<String>,
    #[serde(default)]
    timeout_connect: Option<u64>,
}

impl PeerConnection {
    fn looks_like_resp(&self) -> bool {
        self.namespace_separator.is_some() || self.keys_pattern.is_some() || self.timeout_connect.is_some()
    }
}

pub fn looks_like_resp_export(input: &str) -> bool {
    serde_json::from_str::<Vec<PeerConnection>>(input)
        .is_ok_and(|list| list.iter().any(PeerConnection::looks_like_resp))
}

pub fn parse_resp_connections(input: &str) -> Result<PeerImport, StorageError> {
    if input.len() > MAX_RESP_BYTES {
        return Err(refused("the file is too large to be a RESP.app connection export"));
    }
    let connections = serde_json::from_str::<Vec<PeerConnection>>(input)
        .map_err(|_| refused("the file is not a RESP.app connection export"))?;
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
    Ok(PeerImport {
        drafts,
        skipped,
        groups: std::collections::BTreeMap::new(),
    })
}

fn draft_from(peer: &PeerConnection) -> Option<SavedConnection> {
    if !peer.ssh_host.trim().is_empty() {
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
    if peer.ssl {
        draft.use_tls = true;
        draft.tls_mode = Some(TlsMode::VerifyFull);
    }
    Some(draft)
}

fn display_name(peer: &PeerConnection) -> String {
    skipped_label(&peer.name, peer.host.trim())
}

#[cfg(test)]
#[path = "connection_import_resp_tests.rs"]
mod tests;
