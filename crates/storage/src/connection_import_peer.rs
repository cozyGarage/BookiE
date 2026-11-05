use std::collections::BTreeMap;

use serde::Deserialize;
use uuid::Uuid;

use crate::connection_bundle::{BundleBody, BundleConnection, bundle_organization};
use crate::connection_organization::ConnectionOrganization;
use crate::connection_url::default_port;
use crate::connections::SavedConnection;
use crate::error::StorageError;

const MAX_FIELD_LEN: usize = 256;

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
#[serde(untagged)]
pub(crate) enum PeerPort {
    Number(u64),
    Text(String),
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

pub(crate) fn skipped_label(name: &str, fallback: &str) -> String {
    let name = if name.trim().is_empty() { fallback } else { name.trim() };
    name.chars().filter(|c| !c.is_control()).take(MAX_FIELD_LEN).collect()
}

pub(crate) fn refused(message: &str) -> StorageError {
    StorageError::Schema(message.to_string())
}

#[cfg(test)]
#[path = "connection_import_peer_tests.rs"]
mod tests;
