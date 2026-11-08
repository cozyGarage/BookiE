use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connections::SavedConnection;
use crate::error::StorageError;
use crate::file_access::{lock_path, read_bounded, write_atomically};

const CURRENT_VERSION: u32 = 1;
const MAX_ORGANIZATION_FILE_BYTES: usize = 4 * 1024 * 1024;

/// Bounds on the organisation index. The file is untrusted input, so
/// every collection a caller can grow has a ceiling and the loader
/// refuses a file that exceeds it rather than pulling it into memory
/// and hoping the UI copes.
pub const MAX_ORGANIZED_CONNECTIONS: usize = 1_000;
pub const MAX_TAGS_PER_CONNECTION: usize = 32;
pub const MAX_LABEL_LEN: usize = 64;
pub const MAX_GROUP_DEPTH: usize = 8;
pub const GROUP_PATH_SEPARATOR: &str = " / ";

#[derive(Debug, Clone, PartialEq)]
pub struct GroupTreeNode {
    pub path: Option<String>,
    pub title: String,
    pub connections: Vec<SavedConnection>,
    pub children: Vec<GroupTreeNode>,
}

/// Grouping, tagging and the favourite flag for one saved connection.
/// Kept beside `connections.json` rather than inside it so the
/// connection record stays the single thing the drivers and the
/// transport layer read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectionOrganization {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "is_not_set")]
    pub favorite: bool,
    /// Colour tag, restricted to the fixed palette in
    /// `connection_color`. A value outside it reads as no colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Keys written by a newer BookiE. Carried through a rewrite so a
    /// downgrade does not strip settings the newer version depends on.
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

fn is_not_set(value: &bool) -> bool {
    !*value
}

impl ConnectionOrganization {
    pub fn new(group: Option<&str>, tags: &[String], favorite: bool) -> Result<Self, StorageError> {
        let group = match group.map(str::trim).filter(|value| !value.is_empty()) {
            None => None,
            Some(value) => Some(validate_group_path(value)?),
        };
        if tags.len() > MAX_TAGS_PER_CONNECTION {
            return Err(StorageError::Schema(format!(
                "a connection is limited to {MAX_TAGS_PER_CONNECTION} tags"
            )));
        }
        let mut normalized: Vec<String> = Vec::with_capacity(tags.len());
        for tag in tags {
            let tag = tag.trim();
            if tag.is_empty() {
                continue;
            }
            let tag = validate_label(tag, "tag")?;
            if normalized.iter().any(|kept| kept.eq_ignore_ascii_case(&tag)) {
                continue;
            }
            normalized.push(tag);
        }
        normalized.sort_by_key(|tag| tag.to_lowercase());
        Ok(Self {
            group,
            tags: normalized,
            favorite,
            color: None,
            extra: serde_json::Map::new(),
        })
    }

    /// Set the colour tag. A name outside the palette clears it rather
    /// than storing text that would later reach a CSS class name.
    pub fn with_color(mut self, color: Option<&str>) -> Self {
        self.color = color
            .and_then(crate::connection_color::connection_color)
            .map(str::to_owned);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.group.is_none() && self.tags.is_empty() && !self.favorite && self.color.is_none() && self.extra.is_empty()
    }

    fn sanitized(self) -> Self {
        let group = self
            .group
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| validate_group_path(value).ok());
        let mut tags: Vec<String> = Vec::new();
        for tag in &self.tags {
            let tag = tag.trim();
            if tag.is_empty() || tag.chars().count() > MAX_LABEL_LEN {
                continue;
            }
            if tags.iter().any(|kept| kept.eq_ignore_ascii_case(tag)) {
                continue;
            }
            if tags.len() == MAX_TAGS_PER_CONNECTION {
                break;
            }
            tags.push(tag.to_owned());
        }
        tags.sort_by_key(|tag| tag.to_lowercase());
        Self {
            group,
            tags,
            favorite: self.favorite,
            color: self
                .color
                .as_deref()
                .and_then(crate::connection_color::connection_color)
                .map(str::to_owned),
            extra: self.extra,
        }
    }
}

/// Organisation records keyed by saved-connection id. An id with no
/// entry reads as the default (no group, no tags, not a favourite).
#[derive(Debug, Clone, Default)]
pub struct ConnectionOrganizationIndex {
    entries: BTreeMap<Uuid, ConnectionOrganization>,
    baseline: BTreeMap<Uuid, ConnectionOrganization>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OrganizationFile {
    version: u32,
    #[serde(default)]
    connections: BTreeMap<Uuid, ConnectionOrganization>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl PartialEq for ConnectionOrganizationIndex {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl ConnectionOrganizationIndex {
    pub fn get(&self, id: Uuid) -> ConnectionOrganization {
        self.entries.get(&id).cloned().unwrap_or_default()
    }

    pub fn is_favorite(&self, id: Uuid) -> bool {
        self.entries.get(&id).is_some_and(|entry| entry.favorite)
    }

    pub fn set(&mut self, id: Uuid, organization: ConnectionOrganization) -> Result<(), StorageError> {
        if organization.is_empty() {
            self.entries.remove(&id);
            return Ok(());
        }
        if !self.entries.contains_key(&id) && self.entries.len() >= MAX_ORGANIZED_CONNECTIONS {
            return Err(StorageError::Schema(format!(
                "connection organisation is limited to {MAX_ORGANIZED_CONNECTIONS} connections"
            )));
        }
        self.entries.insert(id, organization);
        Ok(())
    }

    pub fn set_favorite(&mut self, id: Uuid, favorite: bool) -> Result<(), StorageError> {
        let mut entry = self.get(id);
        entry.favorite = favorite;
        self.set(id, entry)
    }

    pub fn remove(&mut self, id: Uuid) {
        self.entries.remove(&id);
    }

    /// Drop entries whose connection no longer exists. Called after a
    /// delete so the sidecar cannot grow without bound as connections
    /// come and go.
    pub fn retain_known(&mut self, connections: &[SavedConnection]) {
        self.entries
            .retain(|id, _| connections.iter().any(|connection| &connection.id == id));
    }

    pub fn groups(&self) -> Vec<String> {
        distinct_labels(self.entries.values().filter_map(|entry| entry.group.as_deref()))
    }

    pub fn group_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        for entry in self.entries.values() {
            let Some(group) = entry.group.as_deref() else {
                continue;
            };
            let segments = parse_group_path(group);
            let mut path = String::new();
            for (index, segment) in segments.iter().enumerate() {
                if index > 0 {
                    path.push_str(GROUP_PATH_SEPARATOR);
                }
                path.push_str(segment);
                paths.push(path.clone());
            }
        }
        distinct_labels(paths.iter().map(String::as_str))
    }

    pub fn tags(&self) -> Vec<String> {
        distinct_labels(
            self.entries
                .values()
                .flat_map(|entry| entry.tags.iter().map(String::as_str)),
        )
    }
}

pub async fn load_organization() -> Result<ConnectionOrganizationIndex, StorageError> {
    load_from(&organization_path()?).await
}

pub async fn save_organization(index: &ConnectionOrganizationIndex) -> Result<(), StorageError> {
    save_to(&organization_path()?, index).await
}

pub(crate) async fn load_from(path: &Path) -> Result<ConnectionOrganizationIndex, StorageError> {
    let Some(file) = load_file(path).await? else {
        return Ok(ConnectionOrganizationIndex::default());
    };
    let entries = sanitized_entries(file.connections)?;
    Ok(ConnectionOrganizationIndex {
        baseline: entries.clone(),
        entries,
    })
}

async fn load_file(path: &Path) -> Result<Option<OrganizationFile>, StorageError> {
    let Some(bytes) = read_bounded(path, MAX_ORGANIZATION_FILE_BYTES).await? else {
        return Ok(None);
    };
    let file: OrganizationFile = serde_json::from_slice(&bytes)?;
    if file.version != CURRENT_VERSION {
        return Err(StorageError::Schema(format!(
            "connection-organization.json version {} not supported (expected {})",
            file.version, CURRENT_VERSION,
        )));
    }
    if file.connections.len() > MAX_ORGANIZED_CONNECTIONS {
        return Err(StorageError::Schema(format!(
            "connection organisation is limited to {MAX_ORGANIZED_CONNECTIONS} connections"
        )));
    }
    Ok(Some(file))
}

fn sanitized_entries(
    entries: BTreeMap<Uuid, ConnectionOrganization>,
) -> Result<BTreeMap<Uuid, ConnectionOrganization>, StorageError> {
    if entries.len() > MAX_ORGANIZED_CONNECTIONS {
        return Err(StorageError::Schema(format!(
            "connection organisation is limited to {MAX_ORGANIZED_CONNECTIONS} connections"
        )));
    }
    Ok(entries
        .into_iter()
        .map(|(id, entry)| (id, entry.sanitized()))
        .filter(|(_, entry)| !entry.is_empty())
        .collect())
}

pub(crate) async fn save_to(path: &Path, index: &ConnectionOrganizationIndex) -> Result<(), StorageError> {
    let _guard = lock_path(path).await?;
    let latest = load_file(path).await?;
    let (mut entries, extra) = match latest {
        Some(file) => (sanitized_entries(file.connections)?, file.extra),
        None => (BTreeMap::new(), serde_json::Map::new()),
    };
    for id in index.baseline.keys() {
        if !index.entries.contains_key(id) {
            entries.remove(id);
        }
    }
    for (id, desired) in &index.entries {
        let baseline = index.baseline.get(id);
        if baseline == Some(desired) {
            continue;
        }
        let mut merged = entries.get(id).cloned().unwrap_or_else(|| desired.clone());
        if baseline.is_none_or(|entry| entry.group != desired.group) {
            merged.group.clone_from(&desired.group);
        }
        if baseline.is_none_or(|entry| entry.tags != desired.tags) {
            merged.tags.clone_from(&desired.tags);
        }
        if baseline.is_none_or(|entry| entry.favorite != desired.favorite) {
            merged.favorite = desired.favorite;
        }
        if baseline.is_none_or(|entry| entry.color != desired.color) {
            merged.color.clone_from(&desired.color);
        }
        if merged.is_empty() {
            entries.remove(id);
        } else {
            entries.insert(*id, merged);
        }
    }
    if entries.len() > MAX_ORGANIZED_CONNECTIONS {
        return Err(StorageError::Schema(format!(
            "connection organisation is limited to {MAX_ORGANIZED_CONNECTIONS} connections"
        )));
    }
    let file = OrganizationFile {
        version: CURRENT_VERSION,
        connections: entries,
        extra,
    };
    let json = serde_json::to_vec_pretty(&file)?;
    if json.len() > MAX_ORGANIZATION_FILE_BYTES {
        return Err(StorageError::TooLarge {
            got: json.len(),
            limit: MAX_ORGANIZATION_FILE_BYTES,
        });
    }
    write_atomically(path, &json).await
}

fn organization_path() -> Result<PathBuf, StorageError> {
    crate::config_path("connection-organization.json")
}

/// Fold labels that differ only in case into one entry and order the
/// result. The representative is the lowest by `Ord` so the list does
/// not change shape with the storage order of the entries behind it.
fn distinct_labels<'a>(labels: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut folded: BTreeMap<String, String> = BTreeMap::new();
    for label in labels {
        let key = label.to_lowercase();
        match folded.get_mut(&key) {
            Some(kept) if label < kept.as_str() => *kept = label.to_owned(),
            Some(_) => {}
            None => {
                folded.insert(key, label.to_owned());
            }
        }
    }
    folded.into_values().collect()
}

fn validate_label(value: &str, field: &str) -> Result<String, StorageError> {
    if value.chars().count() > MAX_LABEL_LEN {
        return Err(StorageError::Schema(format!(
            "a {field} is limited to {MAX_LABEL_LEN} characters"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(StorageError::Schema(format!(
            "a {field} cannot contain control characters"
        )));
    }
    Ok(value.to_owned())
}

/// Does this connection match the filter box? Bare words match the
/// name, group, tags and driver. `group:`, `tag:` and `driver:` narrow
/// to one field, and `is:favorite` keeps only favourites. Every term
/// must match, so terms narrow rather than widen.
pub fn connection_matches_filter(
    connection: &SavedConnection,
    organization: &ConnectionOrganization,
    filter: &str,
) -> bool {
    filter
        .split_whitespace()
        .all(|term| term_matches(connection, organization, term))
}

fn term_matches(connection: &SavedConnection, organization: &ConnectionOrganization, term: &str) -> bool {
    let term = term.to_lowercase();
    if let Some(needle) = term.strip_prefix("group:") {
        return !needle.is_empty()
            && organization
                .group
                .as_deref()
                .is_some_and(|group| group.to_lowercase().contains(needle));
    }
    if let Some(needle) = term.strip_prefix("tag:") {
        return !needle.is_empty() && organization.tags.iter().any(|tag| tag.to_lowercase().contains(needle));
    }
    if let Some(needle) = term.strip_prefix("driver:") {
        return !needle.is_empty() && connection.driver_id.to_lowercase().contains(needle);
    }
    if term == "is:favorite" {
        return organization.favorite;
    }
    connection.name.to_lowercase().contains(&term)
        || connection.driver_id.to_lowercase().contains(&term)
        || organization
            .group
            .as_deref()
            .is_some_and(|group| group.to_lowercase().contains(&term))
        || organization.tags.iter().any(|tag| tag.to_lowercase().contains(&term))
}

/// Filter and order a connection list for display: favourites first,
/// then grouped connections by group name, then ungrouped, with names
/// breaking every remaining tie.
pub fn arrange_connections(
    connections: &[SavedConnection],
    index: &ConnectionOrganizationIndex,
    filter: &str,
) -> Vec<SavedConnection> {
    let mut matched: Vec<SavedConnection> = connections
        .iter()
        .filter(|connection| connection_matches_filter(connection, &index.get(connection.id), filter))
        .cloned()
        .collect();
    matched.sort_by(|left, right| {
        let left_entry = index.get(left.id);
        let right_entry = index.get(right.id);
        right_entry
            .favorite
            .cmp(&left_entry.favorite)
            .then_with(|| group_key(&left_entry).cmp(&group_key(&right_entry)))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    matched
}

fn group_key(entry: &ConnectionOrganization) -> (bool, String) {
    match entry.group.as_deref() {
        Some(group) => (false, group.to_lowercase()),
        None => (true, String::new()),
    }
}

pub fn parse_group_path(path: &str) -> Vec<String> {
    path.split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn join_group_path(segments: &[String]) -> Option<String> {
    let cleaned: Vec<String> = segments
        .iter()
        .map(|segment| segment.trim())
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned.join(GROUP_PATH_SEPARATOR))
}

pub fn validate_group_path(path: &str) -> Result<String, StorageError> {
    let segments = parse_group_path(path);
    if segments.is_empty() {
        return Err(StorageError::Schema("a group cannot be empty".into()));
    }
    if segments.len() > MAX_GROUP_DEPTH {
        return Err(StorageError::Schema(format!(
            "a group path is limited to {MAX_GROUP_DEPTH} levels"
        )));
    }
    for segment in &segments {
        validate_label(segment, "group")?;
    }
    join_group_path(&segments).ok_or_else(|| StorageError::Schema("a group cannot be empty".into()))
}

pub fn connection_group_tree(arranged: &[SavedConnection], index: &ConnectionOrganizationIndex) -> Vec<GroupTreeNode> {
    let mut roots: Vec<GroupTreeNode> = Vec::new();
    for saved in arranged {
        insert_tree_connection(&mut roots, index.get(saved.id).group.as_deref(), saved.clone());
    }
    roots
}

fn insert_tree_connection(roots: &mut Vec<GroupTreeNode>, group: Option<&str>, saved: SavedConnection) {
    let Some(group) = group else {
        push_tree_connection(roots, None, String::new(), saved);
        return;
    };
    let segments = parse_group_path(group);
    if segments.is_empty() {
        push_tree_connection(roots, None, String::new(), saved);
        return;
    }
    insert_at_segments(roots, &segments, 0, String::new(), saved);
}

fn insert_at_segments(
    nodes: &mut Vec<GroupTreeNode>,
    segments: &[String],
    index: usize,
    parent_path: String,
    saved: SavedConnection,
) {
    let segment = &segments[index];
    let path = if parent_path.is_empty() {
        segment.clone()
    } else {
        format!("{parent_path}{GROUP_PATH_SEPARATOR}{segment}")
    };
    let position = nodes
        .iter()
        .position(|node| node.path.as_deref() == Some(path.as_str()))
        .unwrap_or_else(|| {
            nodes.push(GroupTreeNode {
                path: Some(path.clone()),
                title: segment.clone(),
                connections: Vec::new(),
                children: Vec::new(),
            });
            nodes.len() - 1
        });
    if index + 1 == segments.len() {
        nodes[position].connections.push(saved);
        return;
    }
    insert_at_segments(&mut nodes[position].children, segments, index + 1, path, saved);
}

fn push_tree_connection(roots: &mut Vec<GroupTreeNode>, path: Option<String>, title: String, saved: SavedConnection) {
    match roots.iter().position(|node| node.path == path) {
        Some(position) => roots[position].connections.push(saved),
        None => roots.push(GroupTreeNode {
            path,
            title,
            connections: vec![saved],
            children: Vec::new(),
        }),
    }
}

#[cfg(test)]
#[path = "connection_organization_tests.rs"]
mod tests;
