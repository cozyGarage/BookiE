use std::collections::BTreeMap;

use serde::Deserialize;

use uuid::Uuid;

use crate::connection_bundle::{BundleBody, BundleConnection, bundle_organization};
use crate::connection_organization::ConnectionOrganization;
use crate::connection_url::{default_port, draft_connection, validate_host};
use crate::connections::SavedConnection;
use crate::error::StorageError;

pub const MAX_DBEAVER_BYTES: usize = 4 * 1024 * 1024;
const MAX_DBEAVER_CONNECTIONS: usize = 1_000;
const MAX_FIELD_LEN: usize = 256;

const MAX_FOLDER_DEPTH: usize = 8;

#[derive(Debug)]
pub struct PeerImport {
    pub drafts: Vec<SavedConnection>,
    pub skipped: Vec<String>,
    pub groups: BTreeMap<Uuid, String>,
}

impl PeerImport {
    pub fn into_body(self) -> BundleBody {
        let organization = self
            .groups
            .iter()
            .filter_map(|(id, group)| {
                let entry = ConnectionOrganization::new(Some(group), &[], false).ok()?;
                Some((*id, bundle_organization(&entry)))
            })
            .collect();
        BundleBody {
            connections: self.drafts.iter().map(BundleConnection::from_saved).collect(),
            organization,
            ..BundleBody::default()
        }
    }
}

#[derive(Deserialize)]
struct Sources {
    connections: BTreeMap<String, PeerConnection>,
    #[serde(default)]
    folders: BTreeMap<String, PeerFolder>,
}

#[derive(Default, Deserialize)]
struct PeerFolder {
    #[serde(default)]
    parent: String,
}

#[derive(Deserialize)]
struct PeerConnection {
    #[serde(default)]
    provider: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    folder: String,
    #[serde(default)]
    configuration: PeerConfiguration,
}

#[derive(Default, Deserialize)]
struct PeerConfiguration {
    #[serde(default)]
    host: String,
    #[serde(default)]
    port: Option<PeerPort>,
    #[serde(default)]
    database: String,
    #[serde(default)]
    user: String,
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum PeerPort {
    Number(u64),
    Text(String),
}

pub fn parse_dbeaver_data_sources(input: &str) -> Result<PeerImport, StorageError> {
    if input.len() > MAX_DBEAVER_BYTES {
        return Err(refused("the file is too large to be a DBeaver connection list"));
    }
    let sources: Sources = serde_json::from_str(input)
        .map_err(|_| refused("the file is not a DBeaver data-sources.json connection list"))?;
    if sources.connections.is_empty() {
        return Err(refused("the file lists no connections"));
    }
    if sources.connections.len() > MAX_DBEAVER_CONNECTIONS {
        return Err(refused("the file lists too many connections"));
    }
    let mut drafts = Vec::new();
    let mut skipped = Vec::new();
    let mut groups = BTreeMap::new();
    for (key, peer) in &sources.connections {
        match draft_from(peer) {
            Some(draft) => {
                if let Some(group) = group_name(&sources.folders, &peer.folder) {
                    groups.insert(draft.id, group);
                }
                drafts.push(draft);
            }
            None => skipped.push(display_name(peer, key)),
        }
    }
    if drafts.is_empty() {
        return Err(refused("no connection in the file uses an engine BookiE supports"));
    }
    drafts.sort_by(|a, b| a.name.cmp(&b.name));
    skipped.sort();
    Ok(PeerImport {
        drafts,
        skipped,
        groups,
    })
}

fn group_name(folders: &BTreeMap<String, PeerFolder>, folder: &str) -> Option<String> {
    let mut names: Vec<&str> = Vec::new();
    let mut current = folder.trim();
    while !current.is_empty() && names.len() < MAX_FOLDER_DEPTH {
        names.push(current);
        current = folders.get(current).map_or("", |parent| parent.parent.trim());
    }
    names.reverse();
    (!names.is_empty()).then(|| names.join(" / "))
}

fn draft_from(peer: &PeerConnection) -> Option<SavedConnection> {
    let configuration = &peer.configuration;
    let driver_id = engine(&peer.provider, &configuration.url)?;
    let host = validate_host(&configuration.host).ok()?;
    let port = port(configuration.port.as_ref(), driver_id)?;
    let username = field(&configuration.user)?;
    let database = field(&configuration.database)?;
    let name = if peer.name.trim().is_empty() {
        host.clone()
    } else {
        field(peer.name.trim())?
    };
    Some(draft_connection(driver_id, name, host, port, database, username))
}

fn engine(provider: &str, url: &str) -> Option<&'static str> {
    let url = url.to_ascii_lowercase();
    let by_url = [
        ("jdbc:postgresql:", "postgres"),
        ("jdbc:mysql:", "mysql"),
        ("jdbc:mariadb:", "mysql"),
        ("jdbc:sqlserver:", "mssql"),
        ("jdbc:clickhouse:", "clickhouse"),
    ]
    .into_iter()
    .find(|(prefix, _)| url.starts_with(prefix))
    .map(|(_, driver)| driver);
    by_url.or(match provider.to_ascii_lowercase().as_str() {
        "postgresql" => Some("postgres"),
        "mysql" | "mariadb" => Some("mysql"),
        "sqlserver" | "mssql" => Some("mssql"),
        "clickhouse" => Some("clickhouse"),
        "mongodb" => Some("mongodb"),
        "redis" => Some("redis"),
        _ => None,
    })
}

pub(crate) fn port(port: Option<&PeerPort>, driver_id: &str) -> Option<u16> {
    let parsed = match port {
        None => return default_port(driver_id),
        Some(PeerPort::Number(number)) => u16::try_from(*number).ok()?,
        Some(PeerPort::Text(text)) if text.trim().is_empty() => return default_port(driver_id),
        Some(PeerPort::Text(text)) => text.trim().parse::<u16>().ok()?,
    };
    (parsed != 0).then_some(parsed)
}

pub(crate) fn field(value: &str) -> Option<String> {
    (value.len() <= MAX_FIELD_LEN && !value.chars().any(char::is_control)).then(|| value.to_string())
}

fn display_name(peer: &PeerConnection, key: &str) -> String {
    let name = if peer.name.trim().is_empty() {
        key
    } else {
        peer.name.trim()
    };
    name.chars().filter(|c| !c.is_control()).take(MAX_FIELD_LEN).collect()
}

fn refused(message: &str) -> StorageError {
    StorageError::Schema(message.to_string())
}

#[cfg(test)]
#[path = "connection_import_dbeaver_tests.rs"]
mod tests;
