use std::collections::HashMap;
use std::sync::Arc;

use bookie_results::highlight_match::HighlightRuleSet;
use uuid::Uuid;

use super::config_io::xdg_config_path;
use super::state_file::StateFile;

const FILE: &str = "highlight_settings.json";

type Tables = HashMap<String, HighlightRuleSet>;
type Schemas = HashMap<String, Tables>;
type Connections = HashMap<String, Schemas>;

#[derive(Clone)]
pub struct HighlightSettingsStore {
    file: Arc<StateFile<Connections>>,
}

fn schema_key(schema: Option<&str>) -> String {
    schema.unwrap_or("").to_string()
}

impl HighlightSettingsStore {
    pub fn open() -> Option<Self> {
        Some(Self::load_at(xdg_config_path(FILE)?))
    }

    fn load_at(path: std::path::PathBuf) -> Self {
        Self {
            file: Arc::new(StateFile::load(path)),
        }
    }

    pub fn flush(&self) -> Result<(), String> {
        self.file.flush()
    }

    pub fn load(&self, connection_id: Uuid, schema: Option<&str>, table: &str) -> HighlightRuleSet {
        self.file
            .read(|map| {
                map.get(&connection_id.to_string())
                    .and_then(|schemas| schemas.get(&schema_key(schema)))
                    .and_then(|tables| tables.get(table))
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    pub fn save(
        &self,
        connection_id: Uuid,
        schema: Option<&str>,
        table: &str,
        set: HighlightRuleSet,
    ) -> Result<(), String> {
        self.file.update(|map| {
            if set.is_empty() {
                remove_entry(map, connection_id, schema, table);
            } else {
                map.entry(connection_id.to_string())
                    .or_default()
                    .entry(schema_key(schema))
                    .or_default()
                    .insert(table.to_string(), set);
            }
        })
    }
}

fn remove_entry(map: &mut Connections, connection_id: Uuid, schema: Option<&str>, table: &str) {
    let connection = connection_id.to_string();
    let schema = schema_key(schema);
    let Some(schemas) = map.get_mut(&connection) else {
        return;
    };
    let Some(tables) = schemas.get_mut(&schema) else {
        return;
    };
    tables.remove(table);
    if tables.is_empty() {
        schemas.remove(&schema);
    }
    if schemas.is_empty() {
        map.remove(&connection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::FilterValue;
    use bookie_results::highlight_match::{HighlightOp, HighlightRule, HighlightTarget};

    fn sample_set() -> HighlightRuleSet {
        HighlightRuleSet {
            rules: vec![HighlightRule {
                column: "status".into(),
                op: HighlightOp::Eq,
                value: Some(FilterValue::Single("bad".into())),
                color: "red".into(),
                target: HighlightTarget::Cell,
            }],
        }
    }

    #[test]
    fn round_trip_persists_rules_per_table_and_clear_removes_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = HighlightSettingsStore::load_at(dir.path().join(FILE));
        let connection = Uuid::new_v4();
        store
            .save(connection, Some("public"), "orders", sample_set())
            .expect("save");
        store.flush().expect("flush");
        let loaded = store.load(connection, Some("public"), "orders");
        assert_eq!(loaded, sample_set());
        assert!(store.load(connection, Some("public"), "other").is_empty());
        store
            .save(connection, Some("public"), "orders", HighlightRuleSet::default())
            .expect("clear");
        store.flush().expect("flush clear");
        assert!(store.load(connection, Some("public"), "orders").is_empty());
        let raw = std::fs::read_to_string(dir.path().join(FILE)).unwrap_or_default();
        assert!(!raw.contains("orders"), "cleared tables leave no empty blob: {raw}");
    }
}
