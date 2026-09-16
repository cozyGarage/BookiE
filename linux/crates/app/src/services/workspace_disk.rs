use std::path::Path;

use uuid::Uuid;

use super::config_io::{atomic_write_bytes, atomic_write_json};
use super::workspace_state::{WorkspaceState, WorkspaceTabRecord};

pub fn load(path: &Path, drafts: &Path) -> std::io::Result<WorkspaceState> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(WorkspaceState::default()),
        Err(error) => return Err(error),
    };
    let mut state: WorkspaceState = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
    for (connection, workspace) in &mut state.connections {
        let connection = Uuid::parse_str(connection).map_err(std::io::Error::other)?;
        for tab in &mut workspace.tabs {
            if let WorkspaceTabRecord::Editor {
                query,
                draft_id: Some(id),
            } = tab
            {
                *query = std::fs::read_to_string(drafts.join(connection.to_string()).join(format!("{id}.sql")))?;
            }
        }
    }
    Ok(state)
}

pub fn save(path: &Path, drafts: &Path, state: &WorkspaceState) -> std::io::Result<()> {
    let existing = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if let Some(bytes) = &existing {
        let previous: WorkspaceState = serde_json::from_slice(bytes).map_err(std::io::Error::other)?;
        if previous.connections.values().any(|workspace| {
            workspace
                .tabs
                .iter()
                .any(|tab| matches!(tab, WorkspaceTabRecord::Editor { draft_id: None, .. }))
        }) {
            let backup = path.with_extension("before-drafts.json");
            if !backup.try_exists()? {
                atomic_write_bytes(&backup, bytes)?;
            }
        }
    }
    let mut snapshot = state.clone();
    for (connection, workspace) in &mut snapshot.connections {
        let connection = Uuid::parse_str(connection).map_err(std::io::Error::other)?;
        for tab in &mut workspace.tabs {
            if let WorkspaceTabRecord::Editor { query, draft_id } = tab {
                let id = *draft_id.get_or_insert_with(Uuid::new_v4);
                atomic_write_bytes(
                    &drafts.join(connection.to_string()).join(format!("{id}.sql")),
                    query.as_bytes(),
                )?;
                query.clear();
            }
        }
    }
    atomic_write_json(path, &snapshot)
}

#[cfg(test)]
mod tests {
    use super::super::workspace_state::ConnectionWorkspaceState;
    use super::*;

    #[test]
    fn large_legacy_drafts_migrate_without_truncation_and_keep_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.json");
        let drafts = dir.path().join("drafts");
        let id = Uuid::new_v4().to_string();
        let query = "SELECT 'é';\n".repeat(50_000);
        let mut state = WorkspaceState::default();
        state.connections.insert(
            id.clone(),
            ConnectionWorkspaceState {
                tabs: vec![WorkspaceTabRecord::Editor {
                    query: query.clone(),
                    draft_id: None,
                }],
                active_idx: 0,
            },
        );
        atomic_write_json(&path, &state).unwrap();
        let legacy = std::fs::read(&path).unwrap();
        save(&path, &drafts, &load(&path, &drafts).unwrap()).unwrap();
        assert_eq!(
            std::fs::read(path.with_extension("before-drafts.json")).unwrap(),
            legacy
        );
        assert!(std::fs::metadata(&path).unwrap().len() < 1024);
        let loaded = load(&path, &drafts).unwrap();
        assert!(
            matches!(&loaded.connections[&id].tabs[0], WorkspaceTabRecord::Editor { query: text, draft_id: Some(_) } if text == &query)
        );
        save(&path, &drafts, &loaded).unwrap();
        assert_eq!(
            std::fs::read(path.with_extension("before-drafts.json")).unwrap(),
            legacy
        );
        assert_eq!(std::fs::read_dir(drafts.join(&id)).unwrap().count(), 1);
    }

    #[test]
    fn missing_draft_or_failed_write_does_not_replace_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.json");
        let drafts = dir.path().join("drafts");
        let id = Uuid::new_v4().to_string();
        let mut state = WorkspaceState::default();
        state.connections.insert(
            id,
            ConnectionWorkspaceState {
                tabs: vec![WorkspaceTabRecord::Editor {
                    query: "SELECT 1".into(),
                    draft_id: Some(Uuid::new_v4()),
                }],
                active_idx: 0,
            },
        );
        atomic_write_json(&path, &state).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(load(&path, &drafts).is_err());
        std::fs::write(&drafts, "not a directory").unwrap();
        assert!(save(&path, &drafts, &state).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::write(&path, "broken").unwrap();
        assert!(save(&path, &drafts, &state).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"broken");
    }
}
