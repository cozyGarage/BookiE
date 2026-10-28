use std::collections::HashSet;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::{ConnectionWorkspaceState, Uuid, WorkspaceState, WorkspaceTabRecord};
use crate::config_io::prepare_private_file;

pub(super) struct DraftBatch {
    files: Vec<tempfile::TempPath>,
    published: HashSet<PathBuf>,
}

impl DraftBatch {
    pub(super) fn new(drafts: &Path, state: &mut WorkspaceState) -> Self {
        let mut published = HashSet::new();
        for (connection, workspace) in &mut state.connections {
            for tab in workspace.all_tabs_mut() {
                if let WorkspaceTabRecord::Editor { draft_id: Some(id), .. } = tab {
                    published.insert(drafts.join(connection).join(format!("{id}.sql")));
                }
            }
        }
        Self {
            files: Vec::new(),
            published,
        }
    }

    pub(super) fn stage(
        &mut self,
        connection: Uuid,
        workspace: &mut ConnectionWorkspaceState,
        drafts: &Path,
    ) -> std::io::Result<()> {
        let directory = drafts.join(connection.to_string());
        for tab in workspace.all_tabs_mut() {
            let WorkspaceTabRecord::Editor { query, draft_id, .. } = tab else {
                continue;
            };
            let mut id = draft_id.unwrap_or_else(Uuid::new_v4);
            let original = directory.join(format!("{id}.sql"));
            prepare_private_file(&original)?;
            match std::fs::symlink_metadata(&original) {
                Ok(metadata) if !metadata.file_type().is_file() => {
                    return Err(std::io::Error::other("draft path is not a regular file"));
                }
                Ok(_) => {
                    if matches_contents(&original, query.as_bytes())? {
                        query.clear();
                        *draft_id = Some(id);
                        continue;
                    }
                    id = Uuid::new_v4();
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if self.published.contains(&original) {
                        id = Uuid::new_v4();
                    }
                }
                Err(error) => return Err(error),
            }
            std::fs::create_dir_all(&directory)?;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
            let destination = directory.join(format!("{id}.sql"));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&destination)?;
            self.files.push(tempfile::TempPath::try_from_path(destination)?);
            file.write_all(query.as_bytes())?;
            file.sync_all()?;
            std::fs::File::open(&directory)?.sync_all()?;
            *draft_id = Some(id);
            query.clear();
        }
        Ok(())
    }

    pub(super) fn publish(self, write: impl FnOnce() -> std::io::Result<()>) -> std::io::Result<()> {
        self.retain();
        write()
    }

    fn retain(mut self) {
        for file in &mut self.files {
            file.disable_cleanup(true);
        }
    }
}

fn matches_contents(path: &Path, expected: &[u8]) -> std::io::Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut buffer = [0; 8192];
    let mut offset = 0;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            return Ok(offset == expected.len());
        }
        if expected.get(offset..offset + count) != Some(&buffer[..count]) {
            return Ok(false);
        }
        offset += count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_comparison_is_exact_across_buffer_boundaries_and_lengths() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("draft.sql");
        for body in [String::new(), "é".repeat(9000)] {
            std::fs::write(&path, body.as_bytes()).unwrap();
            assert!(matches_contents(&path, body.as_bytes()).unwrap());
            assert!(!matches_contents(&path, format!("{body}x").as_bytes()).unwrap());
            std::fs::write(&path, format!("{body}x")).unwrap();
            assert!(!matches_contents(&path, body.as_bytes()).unwrap());
        }
    }

    fn editor(draft: Uuid) -> ConnectionWorkspaceState {
        ConnectionWorkspaceState {
            tabs: vec![WorkspaceTabRecord::Editor {
                query: "SELECT new".into(),
                draft_id: Some(draft),
                file: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn staging_never_fills_a_missing_body_referenced_by_the_published_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let drafts = root.path().join("drafts");
        let connection = Uuid::new_v4();
        let missing = Uuid::new_v4();
        let mut previous = WorkspaceState::default();
        previous.connections.insert(connection.to_string(), editor(missing));
        let mut batch = DraftBatch::new(&drafts, &mut previous);
        let mut updated = editor(missing);
        batch.stage(connection, &mut updated, &drafts).unwrap();
        assert!(
            !drafts
                .join(connection.to_string())
                .join(format!("{missing}.sql"))
                .exists()
        );
        assert!(matches!(&updated.tabs[0], WorkspaceTabRecord::Editor { draft_id: Some(id), .. } if *id != missing));
        drop(batch);
        assert_eq!(
            std::fs::read_dir(drafts.join(connection.to_string())).unwrap().count(),
            0
        );
    }

    #[test]
    fn publication_error_after_metadata_commit_keeps_the_referenced_draft_readable() {
        let root = tempfile::tempdir().unwrap();
        let metadata = root.path().join("workspace.json");
        let drafts = root.path().join("drafts");
        let connection = Uuid::new_v4();
        let mut previous = WorkspaceState::default();
        let mut batch = DraftBatch::new(&drafts, &mut previous);
        let mut updated = editor(Uuid::new_v4());
        batch.stage(connection, &mut updated, &drafts).unwrap();
        let mut published = WorkspaceState::default();
        published.connections.insert(connection.to_string(), updated);
        let result = batch.publish(|| {
            super::super::atomic_write_json(&metadata, &published)?;
            Err(std::io::Error::other("publication acknowledgement failed"))
        });
        assert!(result.is_err());
        let loaded = super::super::load_connection(&metadata, &drafts, connection)
            .unwrap()
            .unwrap();
        assert!(matches!(&loaded.tabs[0], WorkspaceTabRecord::Editor { query, .. } if query == "SELECT new"));
    }
}
