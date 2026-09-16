use std::collections::HashMap;
use std::sync::OnceLock;

use uuid::Uuid;

use super::config_io::xdg_config_path;
use super::state_file::StateFile;

type Connections = HashMap<String, HashMap<String, HashMap<String, i32>>>;
static STORE: OnceLock<Option<StateFile<Connections>>> = OnceLock::new();

fn store() -> Option<&'static StateFile<Connections>> {
    STORE
        .get_or_init(|| xdg_config_path("column_widths.json").map(StateFile::load))
        .as_ref()
}

pub fn load(connection_id: Uuid, table: &str, column: &str) -> Option<i32> {
    store()?
        .read(|map| map.get(&connection_id.to_string())?.get(table)?.get(column).copied())
        .flatten()
}

pub fn save(connection_id: Uuid, table: &str, column: &str, width: i32) {
    let Some(store) = store() else {
        return;
    };
    if let Err(error) = store.update(|map| {
        map.entry(connection_id.to_string())
            .or_default()
            .entry(table.to_string())
            .or_default()
            .insert(column.to_string(), width);
    }) {
        tracing::warn!(%error, "column width was not saved");
    }
}

pub fn flush() {
    if let Some(Some(store)) = STORE.get()
        && let Err(error) = store.flush()
    {
        tracing::warn!(%error, "column width flush failed");
    }
}
