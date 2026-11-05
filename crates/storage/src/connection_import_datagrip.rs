use std::collections::HashMap;

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::connection_import_peer::{PeerImport, field, refused, skipped_label};
use crate::connection_organization::ConnectionOrganization;
use crate::connection_url::{default_port, draft_connection, validate_host};
use crate::connections::SavedConnection;
use crate::error::StorageError;

pub const MAX_DATAGRIP_BYTES: usize = 4 * 1024 * 1024;
const MAX_SOURCES: usize = 1_000;

#[derive(Default)]
struct Source {
    name: String,
    uuid: String,
    url: String,
    group: String,
}

#[derive(Default)]
struct Local {
    user: String,
    ssh: bool,
}

pub fn parse_datagrip_data_sources(main: &str, local: Option<&str>) -> Result<PeerImport, StorageError> {
    if main.len() > MAX_DATAGRIP_BYTES || local.is_some_and(|text| text.len() > MAX_DATAGRIP_BYTES) {
        return Err(refused("the file is too large to be a DataGrip data source list"));
    }
    let sources = read_sources(main)?;
    if sources.is_empty() {
        return Err(refused("the file lists no data sources"));
    }
    if sources.len() > MAX_SOURCES {
        return Err(refused("the file lists too many data sources"));
    }
    let locals = local.map(read_locals).transpose()?.unwrap_or_default();
    let mut drafts = Vec::new();
    let mut skipped = Vec::new();
    let mut groups = std::collections::BTreeMap::new();
    for source in &sources {
        match draft_from(source, locals.get(&source.uuid)) {
            Some(draft) => {
                if let Some(group) = group_name(&source.group) {
                    groups.insert(draft.id, group);
                }
                drafts.push(draft);
            }
            None => skipped.push(display_name(source)),
        }
    }
    if drafts.is_empty() {
        return Err(refused("no data source in the file uses an engine BookiE supports"));
    }
    drafts.sort_by(|a, b| a.name.cmp(&b.name));
    skipped.sort();
    Ok(PeerImport {
        drafts,
        skipped,
        groups,
    })
}

fn group_name(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw.split('/').map(str::trim).filter(|part| !part.is_empty()).collect();
    let name = field(&parts.join(" / "))?;
    ConnectionOrganization::new(Some(&name), &[], false).ok()?;
    (!parts.is_empty()).then_some(name)
}

fn walk(xml: &str, mut visit: impl FnMut(&[String], Option<&str>, &[(String, String)])) -> Result<(), StorageError> {
    let bad = || refused("the file is not a DataGrip dataSources.xml");
    let mut reader = Reader::from_str(xml);
    let mut path: Vec<String> = Vec::new();
    let mut attributes: Vec<Vec<(String, String)>> = Vec::new();
    let mut text = String::new();
    loop {
        let event = reader.read_event().map_err(|_| bad())?;
        match &event {
            Event::Text(piece) => text.push_str(&piece.xml_content(XmlVersion::Implicit1_0)),
            Event::CData(piece) => text.push_str(piece.as_ref()),
            Event::GeneralRef(reference) => {
                let name = reference.as_ref().to_owned();
                let resolved = match name.as_str() {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    _ => reference.resolve_char_ref().map_err(|_| bad())?.ok_or_else(bad)?,
                };
                text.push(resolved);
            }
            _ => {
                if !text.trim().is_empty() {
                    visit(&path, Some(text.trim()), attributes.last().map_or(&[], Vec::as_slice));
                }
                text.clear();
            }
        }
        match event {
            Event::Start(element) => {
                path.push(element.local_name().as_ref().to_owned());
                let mut pairs = Vec::new();
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|_| bad())?;
                    let key = attribute.key.local_name().as_ref().to_owned();
                    let value = attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|_| bad())?
                        .into_owned();
                    pairs.push((key, value));
                }
                attributes.push(pairs);
                visit(&path, None, attributes.last().map_or(&[], Vec::as_slice));
            }
            Event::End(_) => {
                path.pop();
                attributes.pop();
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

fn read_sources(xml: &str) -> Result<Vec<Source>, StorageError> {
    let mut sources: Vec<Source> = Vec::new();
    walk(xml, |path, text, attributes| {
        let Some(depth) = path.iter().position(|name| name == "data-source") else {
            return;
        };
        let relative = &path[depth + 1..];
        if relative.is_empty() && text.is_none() {
            let mut source = Source::default();
            for (key, value) in attributes {
                match key.as_str() {
                    "name" => source.name.clone_from(value),
                    "uuid" => source.uuid.clone_from(value),
                    "group" => source.group.clone_from(value),
                    _ => {}
                }
            }
            sources.push(source);
        } else if relative == ["jdbc-url"]
            && let Some(text) = text
            && !text.is_empty()
            && let Some(source) = sources.last_mut()
        {
            source.url = text.to_string();
        }
    })?;
    Ok(sources)
}

fn read_locals(xml: &str) -> Result<HashMap<String, Local>, StorageError> {
    let mut locals: HashMap<String, Local> = HashMap::new();
    let mut current = String::new();
    walk(xml, |path, text, attributes| {
        let Some(depth) = path.iter().position(|name| name == "data-source") else {
            return;
        };
        let relative = &path[depth + 1..];
        match text {
            None if relative.is_empty() => {
                current = attributes
                    .iter()
                    .find(|(key, _)| key == "uuid")
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default();
                locals.entry(current.clone()).or_default();
            }
            Some(text) => {
                if let Some(local) = locals.get_mut(&current) {
                    if relative == ["user-name"] {
                        local.user = text.to_string();
                    } else if relative == ["ssh-properties", "enabled"] {
                        local.ssh = text == "true";
                    }
                }
            }
            None => {}
        }
    })?;
    Ok(locals)
}

fn draft_from(source: &Source, local: Option<&Local>) -> Option<SavedConnection> {
    if local.is_some_and(|local| local.ssh) {
        return None;
    }
    let (driver_id, host, port, database) = jdbc_target(&source.url)?;
    let host = validate_host(host).ok()?;
    let port = match port {
        Some(text) => text.parse::<u16>().ok().filter(|port| *port != 0)?,
        None => default_port(driver_id)?,
    };
    let username = field(local.map_or("", |local| local.user.as_str()))?;
    let database = field(database)?;
    let name = if source.name.trim().is_empty() {
        host.clone()
    } else {
        field(source.name.trim())?
    };
    Some(draft_connection(driver_id, name, host, port, database, username))
}

fn jdbc_target(url: &str) -> Option<(&'static str, &str, Option<&str>, &str)> {
    let lower = url.to_ascii_lowercase();
    let (driver_id, prefix) = [
        ("postgres", "jdbc:postgresql:"),
        ("mysql", "jdbc:mysql:"),
        ("mysql", "jdbc:mariadb:"),
        ("mssql", "jdbc:sqlserver:"),
        ("clickhouse", "jdbc:clickhouse:"),
    ]
    .into_iter()
    .find(|(_, prefix)| lower.starts_with(prefix))?;
    let rest = url[prefix.len()..].strip_prefix("//")?;
    let end = rest.find(['/', '?', ';']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.contains([',', '@', '[']) {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    let database = if driver_id == "mssql" {
        tail.trim_start_matches(';')
            .split(';')
            .find_map(|property| {
                let (key, value) = property.split_once('=')?;
                matches!(key.to_ascii_lowercase().as_str(), "databasename" | "database").then_some(value)
            })
            .unwrap_or("")
    } else {
        tail.strip_prefix('/')
            .map_or("", |path| path.split(['?', ';']).next().unwrap_or(""))
    };
    Some((driver_id, host, port, database))
}

fn display_name(source: &Source) -> String {
    skipped_label(&source.name, &source.uuid)
}

#[cfg(test)]
#[path = "connection_import_datagrip_tests.rs"]
mod tests;
