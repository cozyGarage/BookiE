use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::connection_import_dbeaver::{PeerImport, PeerPort, field, port};
use crate::connection_url::{draft_connection, validate_host};
use crate::connections::SavedConnection;
use crate::error::StorageError;
use bookie_core::TlsMode;

pub const MAX_NAVICAT_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 1_000;

#[derive(Default)]
struct PeerConnection {
    name: String,
    kind: String,
    host: String,
    port: String,
    database: String,
    username: String,
    ssh: bool,
    ssl: bool,
}

pub fn parse_navicat_connections(xml: &str) -> Result<PeerImport, StorageError> {
    if xml.len() > MAX_NAVICAT_BYTES {
        return Err(refused("the file is too large to be a Navicat connection export"));
    }
    let connections = read_connections(xml)?;
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
            "no connection in the file can be imported as a direct connection to a supported engine",
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

fn read_connections(xml: &str) -> Result<Vec<PeerConnection>, StorageError> {
    let bad = || refused("the file is not a Navicat connection export");
    let mut reader = Reader::from_str(xml);
    let mut root_seen = false;
    let mut connections = Vec::new();
    loop {
        match reader.read_event().map_err(|_| bad())? {
            Event::Start(element) | Event::Empty(element) => match element.local_name().as_ref() {
                "Connections" => root_seen = true,
                "Connection" if root_seen => connections.push(read_attributes(&element).ok_or_else(bad)?),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    if root_seen { Ok(connections) } else { Err(bad()) }
}

fn read_attributes(element: &BytesStart<'_>) -> Option<PeerConnection> {
    let mut peer = PeerConnection::default();
    for attribute in element.attributes() {
        let attribute = attribute.ok()?;
        let value = attribute.normalized_value(XmlVersion::Implicit1_0).ok()?.into_owned();
        match attribute.key.local_name().as_ref() {
            "ConnectionName" => peer.name = value,
            "ConnType" => peer.kind = value,
            "Host" => peer.host = value,
            "Port" => peer.port = value,
            "Database" => peer.database = value,
            "UserName" => peer.username = value,
            "SSH" => peer.ssh = value.eq_ignore_ascii_case("true"),
            "SSL" => peer.ssl = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    Some(peer)
}

fn engine(kind: &str) -> Option<&'static str> {
    match kind.to_ascii_uppercase().as_str() {
        "MYSQL" | "MARIADB" => Some("mysql"),
        "POSTGRESQL" | "POSTGRES" => Some("postgres"),
        "SQLSERVER" | "MSSQL" => Some("mssql"),
        _ => None,
    }
}

fn draft_from(peer: &PeerConnection) -> Option<SavedConnection> {
    if peer.ssh {
        return None;
    }
    let driver_id = engine(&peer.kind)?;
    let host = validate_host(&peer.host).ok()?;
    let peer_port = PeerPort::Text(peer.port.clone());
    let port = port(Some(&peer_port), driver_id)?;
    let username = field(&peer.username)?;
    let database = field(&peer.database)?;
    let name = if peer.name.trim().is_empty() {
        host.clone()
    } else {
        field(peer.name.trim())?
    };
    let mut draft = draft_connection(driver_id, name, host, port, database, username);
    if peer.ssl {
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
#[path = "connection_import_navicat_tests.rs"]
mod tests;
