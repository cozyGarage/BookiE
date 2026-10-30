use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use uuid::Uuid;

use super::config_io::xdg_config_path;
use super::state_file::StateFile;

type Connections = HashMap<String, HashMap<String, HashMap<String, i32>>>;
type HiddenColumns = HashMap<String, HashMap<String, Vec<String>>>;

pub const MIN_COLUMN_WIDTH: i32 = 24;
pub const MAX_COLUMN_WIDTH: i32 = 4000;

#[derive(Clone)]
pub struct ColumnWidthStore {
    file: Arc<StateFile<Connections>>,
    hidden: Arc<StateFile<HiddenColumns>>,
    order: Arc<StateFile<HiddenColumns>>,
}

pub fn saved_column_order(names: &[String], saved: &[String]) -> Vec<usize> {
    let mut order: Vec<usize> = Vec::with_capacity(names.len());
    for name in saved {
        let Some(index) = names.iter().position(|candidate| candidate == name) else {
            continue;
        };
        if !order.contains(&index) {
            order.push(index);
        }
    }
    let missing: Vec<usize> = (0..names.len()).filter(|index| !order.contains(index)).collect();
    order.extend(missing);
    order
}

pub fn may_hide(total: usize, already_hidden: usize) -> bool {
    total > 0 && already_hidden + 1 < total
}

impl ColumnWidthStore {
    pub fn open() -> Option<Self> {
        Some(Self::load_at(
            xdg_config_path("column_widths.json")?,
            xdg_config_path("hidden_columns.json")?,
            xdg_config_path("column_order.json")?,
        ))
    }

    pub fn load_in(dir: &std::path::Path) -> Self {
        Self::load_at(
            dir.join("column_widths.json"),
            dir.join("hidden_columns.json"),
            dir.join("column_order.json"),
        )
    }

    fn load_at(widths: std::path::PathBuf, hidden: std::path::PathBuf, order: std::path::PathBuf) -> Self {
        Self {
            file: Arc::new(StateFile::load(widths)),
            hidden: Arc::new(StateFile::load(hidden)),
            order: Arc::new(StateFile::load(order)),
        }
    }

    pub fn column_order(&self, connection_id: Uuid, table: &str) -> Vec<String> {
        self.order
            .read(|map| map.get(&connection_id.to_string())?.get(table).cloned())
            .flatten()
            .unwrap_or_default()
    }

    pub fn set_column_order(&self, connection_id: Uuid, table: &str, names: Vec<String>) -> Result<(), String> {
        self.order.update(|map| {
            map.entry(connection_id.to_string())
                .or_default()
                .insert(table.to_string(), names);
        })
    }

    pub fn hidden_columns(&self, connection_id: Uuid, table: &str) -> HashSet<String> {
        self.hidden
            .read(|map| {
                map.get(&connection_id.to_string())
                    .and_then(|tables| tables.get(table))
                    .map(|columns| columns.iter().cloned().collect())
            })
            .flatten()
            .unwrap_or_default()
    }

    pub fn set_hidden(&self, connection_id: Uuid, table: &str, column: &str, hidden: bool) -> Result<(), String> {
        self.hidden.update(|map| {
            let columns = map
                .entry(connection_id.to_string())
                .or_default()
                .entry(table.to_string())
                .or_default();
            columns.retain(|name| name != column);
            if hidden {
                columns.push(column.to_string());
            }
        })
    }

    pub fn load(&self, connection_id: Uuid, table: &str, column: &str) -> Option<i32> {
        self.file
            .read(|map| map.get(&connection_id.to_string())?.get(table)?.get(column).copied())
            .flatten()
            .map(|width| width.clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH))
    }

    pub fn save(&self, connection_id: Uuid, table: &str, column: &str, width: i32) -> Result<(), String> {
        self.file.update(|map| {
            map.entry(connection_id.to_string())
                .or_default()
                .entry(table.to_string())
                .or_default()
                .insert(column.to_string(), width.clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH));
        })
    }

    pub fn forget_widths(&self, connection_id: Uuid, table: &str) -> Result<(), String> {
        let key = connection_id.to_string();
        self.file.update(|map| {
            if let Some(tables) = map.get_mut(&key) {
                tables.remove(table);
            }
        })
    }

    pub fn forget_connection(&self, connection_id: Uuid) -> Result<(), String> {
        let key = connection_id.to_string();
        self.file.update(|map| {
            map.remove(&key);
        })?;
        self.hidden.update(|map| {
            map.remove(&key);
        })?;
        self.order.update(|map| {
            map.remove(&key);
        })
    }

    pub fn flush(&self) -> Result<(), String> {
        self.file.flush()?;
        self.hidden.flush()?;
        self.order.flush()
    }
}

#[cfg(test)]
#[path = "column_widths_tests.rs"]
mod tests;
