use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{Uuid, WorkspaceState, WorkspaceTabRecord, atomic_write_bytes, load_metadata};
use crate::config_io::prepare_private_file;

pub(super) struct DraftBatch<'a> {
    metadata: &'a Path,
    drafts: &'a Path,
    previous: HashSet<PathBuf>,
    created: Vec<PathBuf>,
}

impl<'a> DraftBatch<'a> {
    pub(super) fn new(metadata: &'a Path, drafts: &'a Path) -> std::io::Result<Self> {
        Ok(Self {
            metadata,
            drafts,
            previous: references(drafts, load_metadata(metadata)?),
            created: Vec::new(),
        })
    }

    pub(super) fn save(&mut self, connection: Uuid, previous: Option<Uuid>, bytes: &[u8]) -> std::io::Result<Uuid> {
        let directory = self.drafts.join(connection.to_string());
        let mut id = previous.unwrap_or_else(Uuid::new_v4);
        let path = directory.join(format!("{id}.sql"));
        prepare_private_file(&path)?;
        match std::fs::read(&path) {
            Ok(existing) if existing == bytes => return Ok(id),
            Ok(_) => id = Uuid::new_v4(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if self.previous.contains(&path) {
                    id = Uuid::new_v4();
                }
            }
            Err(error) => return Err(error),
        }
        let path = directory.join(format!("{id}.sql"));
        match std::fs::symlink_metadata(&path) {
            Ok(_) => return Err(std::io::Error::other("new draft path already exists")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.created.push(path.clone());
        atomic_write_bytes(&path, bytes)?;
        Ok(id)
    }
}

impl Drop for DraftBatch<'_> {
    fn drop(&mut self) {
        if self.created.is_empty() {
            return;
        }
        let state = match load_metadata(self.metadata) {
            Ok(state) => state,
            Err(error) => {
                tracing::warn!(%error, "unpublished drafts could not be checked; existing files will be preserved");
                return;
            }
        };
        let retained = references(self.drafts, state);
        for path in &self.created {
            if retained.contains(path) {
                continue;
            }
            if let Err(error) = remove_created(path) {
                tracing::warn!(%error, "unpublished workspace draft could not be removed");
            }
        }
    }
}

fn references(drafts: &Path, mut state: WorkspaceState) -> HashSet<PathBuf> {
    let mut paths = HashSet::new();
    for (connection, workspace) in &mut state.connections {
        for tab in workspace.all_tabs_mut() {
            if let WorkspaceTabRecord::Editor { draft_id: Some(id), .. } = tab {
                paths.insert(drafts.join(connection).join(format!("{id}.sql")));
            }
        }
    }
    paths
}

fn remove_created(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => std::fs::remove_file(path),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_state::ConnectionWorkspaceState;

    #[test]
    fn a_published_draft_survives_cleanup_even_when_the_attempt_did_not_acknowledge_it() {
        let root = tempfile::tempdir().unwrap();
        let metadata = root.path().join("workspace.json");
        let drafts = root.path().join("drafts");
        let connection = Uuid::new_v4();
        let mut batch = DraftBatch::new(&metadata, &drafts).unwrap();
        let id = batch.save(connection, None, b"published").unwrap();
        let mut state = WorkspaceState::default();
        state.connections.insert(
            connection.to_string(),
            ConnectionWorkspaceState {
                tabs: vec![WorkspaceTabRecord::Editor {
                    query: String::new(),
                    draft_id: Some(id),
                    file: None,
                }],
                ..Default::default()
            },
        );
        super::super::super::config_io::atomic_write_json(&metadata, &state).unwrap();
        drop(batch);
        assert_eq!(
            std::fs::read(drafts.join(connection.to_string()).join(format!("{id}.sql"))).unwrap(),
            b"published"
        );
    }
}
