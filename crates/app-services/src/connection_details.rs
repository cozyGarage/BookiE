use std::time::Duration;

use bookie_storage::SavedConnection;

const CHIP_VERSION_CHARS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionDetails {
    pub endpoint: String,
    pub database: String,
    pub ssh_hops: usize,
    pub tls: String,
}

impl ConnectionDetails {
    pub fn from_saved(saved: &SavedConnection) -> Self {
        let endpoint = match (&saved.socket_path, &saved.socket_dir) {
            (Some(path), _) => path.display().to_string(),
            (None, Some(dir)) => dir.display().to_string(),
            (None, None) if saved.host.is_empty() => String::new(),
            (None, None) => format!("{}:{}", saved.host, saved.port),
        };
        let ssh_hops = std::iter::successors(saved.ssh.as_ref(), |hop| hop.jump.as_deref()).count();
        let tls = if endpoint.is_empty() {
            "Not used".to_string()
        } else if saved.use_tls {
            saved.tls_mode.unwrap_or_default().label().to_string()
        } else {
            "Off".to_string()
        };
        Self {
            endpoint,
            database: saved.database.clone(),
            ssh_hops,
            tls,
        }
    }

    pub fn tooltip(&self, server_version: Option<&str>, round_trip: Option<Duration>) -> String {
        let mut lines = Vec::new();
        if let Some(version) = server_version.map(str::trim).filter(|version| !version.is_empty()) {
            lines.push(format!("Server: {version}"));
        }
        if !self.endpoint.is_empty() {
            lines.push(format!("Address: {}", self.endpoint));
        }
        if !self.database.is_empty() {
            lines.push(format!("Database: {}", self.database));
        }
        lines.push(match self.ssh_hops {
            0 => "SSH: none".to_string(),
            1 => "SSH: 1 hop".to_string(),
            hops => format!("SSH: {hops} hops"),
        });
        lines.push(format!("TLS: {}", self.tls));
        if let Some(round_trip) = round_trip {
            lines.push(format!("Round trip: {} ms", round_trip.as_millis()));
        }
        lines.join("\n")
    }
}

pub async fn load_connection_details(id: uuid::Uuid) -> Result<Option<ConnectionDetails>, String> {
    let saved = bookie_storage::load_connections()
        .await
        .map_err(|error| error.to_string())?;
    Ok(saved
        .iter()
        .find(|connection| connection.id == id)
        .map(ConnectionDetails::from_saved))
}

pub fn short_server_version(version: &str) -> String {
    let line = version.trim().lines().next().unwrap_or_default();
    let cut = [" (", " on ", ", "]
        .iter()
        .filter_map(|separator| line.find(separator))
        .min()
        .unwrap_or(line.len());
    line[..cut].trim().chars().take(CHIP_VERSION_CHARS).collect()
}

#[cfg(test)]
#[path = "connection_details_tests.rs"]
mod tests;
