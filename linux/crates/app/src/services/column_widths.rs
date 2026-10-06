use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use uuid::Uuid;

use super::config_io::xdg_config_path;
use super::state_file::StateFile;

type Connections = HashMap<String, HashMap<String, HashMap<String, i32>>>;
type HiddenColumns = HashMap<String, HashMap<String, Vec<String>>>;

#[derive(Clone)]
pub struct ColumnWidthStore {
    file: Arc<StateFile<Connections>>,
    hidden: Arc<StateFile<HiddenColumns>>,
}

pub fn may_hide(total: usize, already_hidden: usize) -> bool {
    total > 0 && already_hidden + 1 < total
}

impl ColumnWidthStore {
    pub fn open() -> Option<Self> {
        Some(Self::load_at(
            xdg_config_path("column_widths.json")?,
            xdg_config_path("hidden_columns.json")?,
        ))
    }

    #[cfg(test)]
    pub fn load_in(dir: &std::path::Path) -> Self {
        Self::load_at(dir.join("column_widths.json"), dir.join("hidden_columns.json"))
    }

    fn load_at(widths: std::path::PathBuf, hidden: std::path::PathBuf) -> Self {
        Self {
            file: Arc::new(StateFile::load(widths)),
            hidden: Arc::new(StateFile::load(hidden)),
        }
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
        self.file.flush()?;
        self.hidden.flush()
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
        let first_store = store_in(&first);
        let second_store = store_in(&second);

        first_store.save(id, "users", "name", 240).unwrap();
        first_store.flush().unwrap();

        assert_eq!(first_store.load(id, "users", "name"), Some(240));
        assert_eq!(second_store.load(id, "users", "name"), None);
    }

    fn store_in(dir: &tempfile::TempDir) -> ColumnWidthStore {
        ColumnWidthStore::load_at(dir.path().join("widths.json"), dir.path().join("hidden.json"))
    }

    #[test]
    fn hidden_columns_are_remembered_per_connection_and_table() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let store = store_in(&dir);

        store.set_hidden(a, "users", "email", true).unwrap();

        assert!(store.hidden_columns(a, "users").contains("email"));
        assert!(store.hidden_columns(a, "orders").is_empty());
        assert!(store.hidden_columns(b, "users").is_empty());
    }

    #[test]
    fn showing_a_column_again_removes_it_from_the_hidden_set() {
        let dir = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4();
        let store = store_in(&dir);
        store.set_hidden(id, "users", "email", true).unwrap();
        store.set_hidden(id, "users", "email", false).unwrap();
        assert!(store.hidden_columns(id, "users").is_empty());
    }

    #[test]
    fn hidden_columns_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4();
        let store = store_in(&dir);
        store.set_hidden(id, "users", "email", true).unwrap();
        store.flush().unwrap();

        let reopened = store_in(&dir);
        assert!(reopened.hidden_columns(id, "users").contains("email"));
    }

    #[test]
    fn the_last_visible_column_cannot_be_hidden() {
        assert!(may_hide(3, 1));
        assert!(!may_hide(3, 2));
        assert!(!may_hide(3, 3));
        assert!(!may_hide(1, 0));
        assert!(!may_hide(0, 0));
    }
}
