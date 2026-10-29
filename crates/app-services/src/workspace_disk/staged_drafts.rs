use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use super::{ConnectionWorkspaceState, Uuid, WorkspaceTabRecord};

#[derive(Default)]
pub(super) struct DraftBatch {
    files: Vec<tempfile::TempPath>,
}

impl DraftBatch {
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
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
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

    pub(super) fn retain(mut self) {
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
}
