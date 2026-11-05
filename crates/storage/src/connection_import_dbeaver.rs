use std::collections::BTreeMap;

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::connection_import_peer::{PeerImport, PeerPort, field, port, refused, skipped_label};
use crate::connection_url::{draft_connection, validate_host};
use crate::connections::SavedConnection;
use crate::error::StorageError;

pub const MAX_DBEAVER_BYTES: usize = 4 * 1024 * 1024;
const MAX_DBEAVER_CONNECTIONS: usize = 1_000;

const MAX_FOLDER_DEPTH: usize = 8;

#[derive(Deserialize)]
struct Sources {
    connections: BTreeMap<String, PeerConnection>,
    #[serde(default)]
    folders: Folders,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Folders {
    Map(BTreeMap<String, FolderEntry>),
    Other(IgnoredAny),
}

impl Default for Folders {
    fn default() -> Self {
        Self::Map(BTreeMap::new())
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FolderEntry {
    Object(PeerFolder),
    Other(IgnoredAny),
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

fn parent_of<'a>(folders: &'a Folders, name: &str) -> &'a str {
    match folders {
        Folders::Map(map) => match map.get(name) {
            Some(FolderEntry::Object(folder)) => folder.parent.trim(),
            _ => "",
        },
        Folders::Other(_) => "",
    }
}

fn group_name(folders: &Folders, folder: &str) -> Option<String> {
    let segments: Vec<&str> = folder
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    let mut top = *segments.first()?;
    let mut seen = vec![top];
    let mut ancestors: Vec<&str> = Vec::new();
    while ancestors.len() + segments.len() < MAX_FOLDER_DEPTH {
        let parent = parent_of(folders, top);
        if parent.is_empty() || seen.contains(&parent) {
            break;
        }
        ancestors.push(parent);
        seen.push(parent);
        top = parent;
    }
    ancestors.reverse();
    ancestors.extend(segments);
    let start = ancestors.len().saturating_sub(MAX_FOLDER_DEPTH);
    Some(ancestors[start..].join(" / "))
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

fn display_name(peer: &PeerConnection, key: &str) -> String {
    skipped_label(&peer.name, key)
}

#[cfg(test)]
#[path = "connection_import_dbeaver_tests.rs"]
mod tests;
