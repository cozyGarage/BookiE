use std::collections::HashSet;
use std::path::Path;

use super::{ConnectionWorkspaceState, Uuid, WorkspaceTabRecord};

pub(super) fn remove(drafts: &Path, connection: Uuid, workspace: &mut ConnectionWorkspaceState) -> std::io::Result<()> {
    let retained: HashSet<Uuid> = workspace
        .all_tabs_mut()
        .filter_map(|tab| match tab {
            WorkspaceTabRecord::Editor { draft_id, .. } => *draft_id,
            _ => None,
        })
        .collect();
    let directory = drafts.join(connection.to_string());
    match std::fs::symlink_metadata(&directory) {
        Ok(metadata) if !metadata.file_type().is_dir() => {
            return Err(std::io::Error::other("draft directory is not a directory"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_suffix(".sql"))
            .and_then(|id| Uuid::parse_str(id).ok())
        else {
            continue;
        };
        if !retained.contains(&id)
            && let Err(error) = std::fs::remove_file(entry.path())
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(error);
        }
    }
    Ok(())
}
