use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const OPEN_SESSIONS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenSessionsRecord {
    pub version: u32,
    #[serde(default)]
    pub windows: Vec<Vec<Uuid>>,
}

impl Default for OpenSessionsRecord {
    fn default() -> Self {
        Self {
            version: OPEN_SESSIONS_VERSION,
            windows: Vec::new(),
        }
    }
}

impl OpenSessionsRecord {
    pub fn from_windows(windows: impl IntoIterator<Item = Vec<Uuid>>) -> Self {
        Self {
            version: OPEN_SESSIONS_VERSION,
            windows: windows.into_iter().collect(),
        }
        .sanitized()
    }

    pub fn from_legacy_last_connection(last_connection_id: Option<Uuid>) -> Self {
        match last_connection_id {
            Some(id) => Self::from_windows([vec![id]]),
            None => Self::default(),
        }
    }

    pub fn sanitized(self) -> Self {
        let mut seen = HashSet::new();
        let mut windows = Vec::with_capacity(self.windows.len());
        for window in self.windows {
            let mut keys = Vec::with_capacity(window.len());
            for key in window {
                if seen.insert(key) {
                    keys.push(key);
                }
            }
            if !keys.is_empty() {
                windows.push(keys);
            }
        }
        Self {
            version: self.version,
            windows,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenSessionsLoad {
    Restored(OpenSessionsRecord),
    Empty,
    UnsupportedVersion(u32),
}

pub fn parse_open_sessions(bytes: &[u8]) -> OpenSessionsLoad {
    let Ok(record) = serde_json::from_slice::<OpenSessionsRecord>(bytes) else {
        return OpenSessionsLoad::Empty;
    };
    if record.version != OPEN_SESSIONS_VERSION {
        return OpenSessionsLoad::UnsupportedVersion(record.version);
    }
    let sanitized = record.sanitized();
    if sanitized.windows.is_empty() {
        OpenSessionsLoad::Empty
    } else {
        OpenSessionsLoad::Restored(sanitized)
    }
}

pub fn restore_window_keys(keys: &[Uuid], available: &[Uuid]) -> Vec<Uuid> {
    let available = available.iter().copied().collect::<HashSet<_>>();
    keys.iter().copied().filter(|key| available.contains(key)).collect()
}

pub fn startup_restore_keys(stored: Option<&[u8]>, last_connection_id: Option<Uuid>, available: &[Uuid]) -> Vec<Uuid> {
    let Some(front) = last_connection_id else {
        return Vec::new();
    };
    let stored_window = match stored.map(parse_open_sessions) {
        Some(OpenSessionsLoad::Restored(record)) => record.windows.into_iter().find(|keys| keys.contains(&front)),
        _ => None,
    };
    let window = stored_window.unwrap_or_else(|| vec![front]);
    let mut keys = restore_window_keys(&window, available);
    if let Some(position) = keys.iter().position(|key| *key == front) {
        let front = keys.remove(position);
        keys.push(front);
    }
    keys
}
