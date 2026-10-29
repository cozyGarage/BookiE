#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_app_services::workspace_state::{ConnectionWorkspaceState, WorkspaceStore, WorkspaceTabRecord};
use uuid::Uuid;

fn state(a: Uuid, b: Uuid, first: &str, second: &str) -> ConnectionWorkspaceState {
    ConnectionWorkspaceState {
        tabs: vec![
            WorkspaceTabRecord::Editor {
                query: first.into(),
                draft_id: Some(a),
                file: None,
            },
            WorkspaceTabRecord::Editor {
                query: second.into(),
                draft_id: Some(b),
                file: None,
            },
        ],
        ..Default::default()
    }
}

fn flush(store: &WorkspaceStore) -> Result<(), tablepro_app_services::workspace_state::WorkspaceFlushError> {
    store.flush().recv_timeout(std::time::Duration::from_secs(20)).unwrap()
}

#[test]
fn unchanged_drafts_refuse_symlinks_and_restore_private_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    let drafts = root.path().join("drafts");
    let connection = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let writer = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    writer.save_connection(connection, state(first, second, "first", "second"));
    assert_eq!(flush(&writer), Ok(()));
    let directory = drafts.join(connection.to_string());
    let draft = directory.join(format!("{first}.sql"));
    std::fs::set_permissions(&draft, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
    writer.save_connection(connection, state(first, second, "first", "second"));
    assert_eq!(flush(&writer), Ok(()));
    assert_eq!(std::fs::metadata(&draft).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(
        std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let metadata = std::fs::read(path).unwrap();
    let outside = root.path().join("outside.sql");
    std::fs::write(&outside, "first").unwrap();
    std::fs::remove_file(&draft).unwrap();
    std::os::unix::fs::symlink(&outside, &draft).unwrap();
    for text in ["first", "changed"] {
        writer.save_connection(connection, state(first, second, text, "second"));
        assert!(flush(&writer).is_err());
        assert_eq!(std::fs::read(root.path().join("workspace.json")).unwrap(), metadata);
        assert_eq!(std::fs::read(&outside).unwrap(), b"first");
        assert!(std::fs::symlink_metadata(&draft).unwrap().file_type().is_symlink());
    }
}

#[test]
fn repeated_failed_batches_do_not_accumulate_unpublished_drafts() {
    for metadata_failure in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original.json");
        let path = root.path().join("workspace.json");
        let drafts = root.path().join("drafts");
        let connection = Uuid::new_v4();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let writer = WorkspaceStore::new(Some(original.clone()), drafts.clone());
        writer.save_connection(connection, state(first, second, "old first", "old second"));
        assert_eq!(flush(&writer), Ok(()));
        let directory = drafts.join(connection.to_string());
        let metadata = std::fs::read(&original).unwrap();
        let target = if metadata_failure {
            std::os::unix::fs::symlink(&original, &path).unwrap();
            path
        } else {
            let blocked = directory.join(format!("{second}.sql"));
            std::fs::remove_file(&blocked).unwrap();
            std::fs::create_dir(&blocked).unwrap();
            original.clone()
        };
        let failing = WorkspaceStore::new(Some(target), drafts);
        failing.save_connection(connection, state(first, second, "new first", "new second"));
        for _ in 0..5 {
            assert!(flush(&failing).is_err());
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
            assert_eq!(std::fs::read(&original).unwrap(), metadata);
            assert_eq!(
                std::fs::read(directory.join(format!("{first}.sql"))).unwrap(),
                b"old first"
            );
        }
    }
}

#[test]
fn a_failed_save_does_not_fill_a_missing_draft_in_the_published_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.json");
    let path = root.path().join("workspace.json");
    let drafts = root.path().join("drafts");
    let connection = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let writer = WorkspaceStore::new(Some(original.clone()), drafts.clone());
    writer.save_connection(connection, state(first, second, "old first", "old second"));
    assert_eq!(flush(&writer), Ok(()));
    let directory = drafts.join(connection.to_string());
    let missing = directory.join(format!("{first}.sql"));
    std::fs::remove_file(&missing).unwrap();
    std::os::unix::fs::symlink(&original, &path).unwrap();
    let failing = WorkspaceStore::new(Some(path), drafts);
    failing.save_connection(connection, state(first, second, "new first", "new second"));
    assert!(flush(&failing).is_err());
    assert!(!missing.exists());
    assert_eq!(std::fs::read_dir(directory).unwrap().count(), 1);
}
