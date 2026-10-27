use super::*;

fn workspace(ids: &[Uuid]) -> ConnectionWorkspaceState {
    ConnectionWorkspaceState {
        tabs: ids
            .iter()
            .map(|id| WorkspaceTabRecord::Editor {
                query: format!("SELECT '{id}'"),
                draft_id: Some(*id),
                file: None,
            })
            .collect(),
        ..Default::default()
    }
}

fn save_update(
    path: &Path,
    drafts: &Path,
    id: Uuid,
    workspace: ConnectionWorkspaceState,
    incremental: bool,
) -> std::io::Result<()> {
    if incremental {
        return save_connections(path, drafts, vec![(id, workspace)]);
    }
    let mut state = WorkspaceState::default();
    state.connections.insert(id.to_string(), workspace);
    save(path, drafts, state)
}

#[test]
fn closed_editor_drafts_are_removed_after_either_save_path() {
    for incremental in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.json");
        let drafts = directory.path().join("drafts");
        let id = Uuid::new_v4();
        let kept = Uuid::new_v4();
        let closed = Uuid::new_v4();
        save_update(&path, &drafts, id, workspace(&[kept, closed]), incremental).unwrap();
        let folder = drafts.join(id.to_string());
        let orphan = folder.join(format!("{}.sql", Uuid::new_v4()));
        std::fs::write(&orphan, "obsolete draft").unwrap();
        let unrelated = folder.join("notes.sql");
        std::fs::write(&unrelated, "personal notes").unwrap();
        let outside = directory.path().join("outside.sql");
        std::fs::write(&outside, "keep outside").unwrap();
        let link = folder.join(format!("{}.sql", Uuid::new_v4()));
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        save_update(&path, &drafts, id, workspace(&[kept]), incremental).unwrap();
        assert!(!folder.join(format!("{closed}.sql")).exists());
        assert!(!orphan.exists());
        assert_eq!(
            std::fs::read_to_string(folder.join(format!("{kept}.sql"))).unwrap(),
            format!("SELECT '{kept}'")
        );
        assert_eq!(std::fs::read_to_string(unrelated).unwrap(), "personal notes");
        assert!(std::fs::symlink_metadata(link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(outside).unwrap(), "keep outside");
        save_update(&path, &drafts, id, workspace(&[]), incremental).unwrap();
        assert!(!folder.join(format!("{kept}.sql")).exists());
    }
}

#[test]
fn drafts_in_another_database_and_connection_survive_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.json");
    let drafts = directory.path().join("drafts");
    let id = Uuid::new_v4();
    let other = Uuid::new_v4();
    let retained = Uuid::new_v4();
    let closed = Uuid::new_v4();
    let nested = Uuid::new_v4();
    save_connections(&path, &drafts, vec![(other, workspace(&[retained]))]).unwrap();
    let mut current = workspace(&[retained, closed]);
    current.other_databases.insert("other".into(), workspace(&[nested]));
    save_connections(&path, &drafts, vec![(id, current.clone())]).unwrap();
    current.tabs.pop();
    save_connections(&path, &drafts, vec![(id, current)]).unwrap();
    assert!(!drafts.join(id.to_string()).join(format!("{closed}.sql")).exists());
    for (connection, draft) in [(id, retained), (id, nested), (other, retained)] {
        assert_eq!(
            std::fs::read_to_string(drafts.join(connection.to_string()).join(format!("{draft}.sql"))).unwrap(),
            format!("SELECT '{draft}'")
        );
    }
}

#[test]
fn a_failed_metadata_commit_keeps_drafts_referenced_by_the_previous_workspace() {
    for incremental in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.json");
        let path = directory.path().join("workspace.json");
        let drafts = directory.path().join("drafts");
        let id = Uuid::new_v4();
        let draft = Uuid::new_v4();
        save_update(&original, &drafts, id, workspace(&[draft]), incremental).unwrap();
        std::os::unix::fs::symlink(&original, &path).unwrap();
        let before = std::fs::read(&original).unwrap();
        assert!(save_update(&path, &drafts, id, workspace(&[]), incremental).is_err());
        assert_eq!(std::fs::read(&original).unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(drafts.join(id.to_string()).join(format!("{draft}.sql"))).unwrap(),
            format!("SELECT '{draft}'")
        );
    }
}

#[test]
fn a_symlinked_connection_directory_is_not_cleaned() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.json");
    let drafts = directory.path().join("drafts");
    let outside = directory.path().join("outside");
    std::fs::create_dir_all(&drafts).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let id = Uuid::new_v4();
    let draft = outside.join(format!("{}.sql", Uuid::new_v4()));
    std::fs::write(&draft, "outside draft").unwrap();
    std::os::unix::fs::symlink(&outside, drafts.join(id.to_string())).unwrap();
    assert!(save_connections(&path, &drafts, vec![(id, workspace(&[]))]).is_err());
    assert_eq!(std::fs::read_to_string(draft).unwrap(), "outside draft");
}
