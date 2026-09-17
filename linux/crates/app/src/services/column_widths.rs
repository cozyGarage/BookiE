use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use super::config_io::xdg_config_path;
use super::state_file::StateFile;

type Connections = HashMap<String, HashMap<String, HashMap<String, i32>>>;

#[derive(Clone)]
pub struct ColumnWidthStore {
    file: Arc<StateFile<Connections>>,
}

impl ColumnWidthStore {
    pub fn open() -> Option<Self> {
        Some(Self::load_at(xdg_config_path("column_widths.json")?))
    }

    fn load_at(path: std::path::PathBuf) -> Self {
        Self {
            file: Arc::new(StateFile::load(path)),
        }
    }

    pub fn load(&self, connection_id: Uuid, table: &str, column: &str) -> Option<i32> {
        self.file
            .read(|map| map.get(&connection_id.to_string())?.get(table)?.get(column).copied())
            .flatten()
    }

    pub fn save(&self, connection_id: Uuid, table: &str, column: &str, width: i32) -> Result<(), String> {
        self.file.update(|map| {
            map.entry(connection_id.to_string())
                .or_default()
                .entry(table.to_string())
                .or_default()
                .insert(column.to_string(), width);
        })
    }

    pub fn flush(&self) -> Result<(), String> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separately_opened_stores_do_not_share_state() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4();
        let first_store = ColumnWidthStore::load_at(first.path().join("widths.json"));
        let second_store = ColumnWidthStore::load_at(second.path().join("widths.json"));

        first_store.save(id, "users", "name", 240).unwrap();
        first_store.flush().unwrap();

        assert_eq!(first_store.load(id, "users", "name"), Some(240));
        assert_eq!(second_store.load(id, "users", "name"), None);
    }
}
