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
fn a_failed_draft_batch_preserves_the_previous_first_draft() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    let drafts = root.path().join("drafts");
    let connection = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let store = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    store.save_connection(connection, state(first, second, "old first", "old second"));
    assert_eq!(flush(&store), Ok(()));
    let metadata = std::fs::read(&path).unwrap();
    let directory = drafts.join(connection.to_string());
    let blocked = directory.join(format!("{second}.sql"));
    std::fs::remove_file(&blocked).unwrap();
    std::fs::create_dir(&blocked).unwrap();
    store.save_connection(connection, state(first, second, "new first", "new second"));
    assert!(flush(&store).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), metadata);
    assert_eq!(
        std::fs::read_to_string(directory.join(format!("{first}.sql"))).unwrap(),
        "old first"
    );
    std::fs::remove_dir(&blocked).unwrap();
    std::fs::write(&blocked, "old second").unwrap();
    assert_eq!(flush(&store), Ok(()));
    let reader = WorkspaceStore::new(Some(path), drafts);
    reader.prefetch_connections(&[connection]).unwrap();
    let restored = reader.load_connection(connection).unwrap();
    assert!(matches!(&restored.tabs[0], WorkspaceTabRecord::Editor { query, .. } if query == "new first"));
    assert!(matches!(&restored.tabs[1], WorkspaceTabRecord::Editor { query, .. } if query == "new second"));
    assert_eq!(std::fs::read_dir(directory).unwrap().count(), 2);
}

#[test]
fn an_unreadable_unrequested_connection_does_not_block_the_requested_workspace() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    let drafts = root.path().join("drafts");
    let broken = Uuid::new_v4();
    let healthy = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let writer = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    writer.save_connection(broken, state(first, second, "broken draft", "other"));
    writer.save_connection(healthy, ConnectionWorkspaceState::default());
    assert_eq!(flush(&writer), Ok(()));
    std::fs::write(drafts.join(broken.to_string()).join(format!("{first}.sql")), [0xff]).unwrap();
    let reader = WorkspaceStore::new(Some(path), drafts);
    assert_eq!(reader.prefetch_connections(&[healthy]), Ok(()));
    assert!(reader.load_connection(healthy).is_some());
}

#[test]
fn requested_unreadable_drafts_report_failure_without_blocking_healthy_restoration() {
    for invalid_utf8 in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("workspace.json");
        let drafts = root.path().join("drafts");
        let broken = Uuid::new_v4();
        let healthy = Uuid::new_v4();
        let draft = Uuid::new_v4();
        let writer = WorkspaceStore::new(Some(path.clone()), drafts.clone());
        writer.save_connection(broken, state(draft, Uuid::new_v4(), "broken", "other"));
        writer.save_connection(healthy, state(Uuid::new_v4(), Uuid::new_v4(), "healthy", "other"));
        assert_eq!(flush(&writer), Ok(()));
        let metadata = std::fs::read(&path).unwrap();
        let bad_path = drafts.join(broken.to_string()).join(format!("{draft}.sql"));
        if invalid_utf8 {
            std::fs::write(&bad_path, [0xff]).unwrap();
        } else {
            std::fs::remove_file(&bad_path).unwrap();
            std::fs::create_dir(&bad_path).unwrap();
        }
        let reader = WorkspaceStore::new(Some(path.clone()), drafts);
        assert!(reader.prefetch_connections(&[broken, healthy]).is_err());
        assert!(reader.load_connection(broken).is_none());
        let restored = reader.load_connection(healthy).unwrap();
        assert!(matches!(&restored.tabs[0], WorkspaceTabRecord::Editor { query, .. } if query == "healthy"));
        assert_eq!(std::fs::read(path).unwrap(), metadata);
        if invalid_utf8 {
            assert_eq!(std::fs::read(bad_path).unwrap(), [0xff]);
        } else {
            assert!(bad_path.is_dir());
        }
    }
}

#[test]
fn failed_metadata_publication_preserves_previously_acknowledged_bodies() {
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
    std::os::unix::fs::symlink(&original, &path).unwrap();
    let failing = WorkspaceStore::new(Some(path), drafts.clone());
    failing.save_connection(connection, state(first, second, "new first", "new second"));
    assert!(flush(&failing).is_err());
    let reader = WorkspaceStore::new(Some(original), drafts);
    reader.prefetch_connections(&[connection]).unwrap();
    let restored = reader.load_connection(connection).unwrap();
    assert!(matches!(&restored.tabs[0], WorkspaceTabRecord::Editor { query, .. } if query == "old first"));
    assert!(matches!(&restored.tabs[1], WorkspaceTabRecord::Editor { query, .. } if query == "old second"));
}

#[test]
fn missing_drafts_restore_empty_editors_while_malformed_metadata_refuses_prefetch() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    let drafts = root.path().join("drafts");
    let connection = Uuid::new_v4();
    let missing = Uuid::new_v4();
    let writer = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    writer.save_connection(connection, state(missing, Uuid::new_v4(), "missing", "healthy"));
    assert_eq!(flush(&writer), Ok(()));
    std::fs::remove_file(drafts.join(connection.to_string()).join(format!("{missing}.sql"))).unwrap();
    let reader = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    assert_eq!(reader.prefetch_connections(&[connection]), Ok(()));
    let restored = reader.load_connection(connection).unwrap();
    assert!(matches!(&restored.tabs[0], WorkspaceTabRecord::Editor { query, draft_id: None, .. } if query.is_empty()));
    assert!(matches!(&restored.tabs[1], WorkspaceTabRecord::Editor { query, .. } if query == "healthy"));
    for invalid in ["invalid metadata", r#"{"connections":{"not-a-uuid":{"tabs":[]}}}"#] {
        std::fs::write(&path, invalid).unwrap();
        let fresh = WorkspaceStore::new(Some(path.clone()), drafts.clone());
        assert!(fresh.prefetch_connections(&[connection]).is_err());
        assert!(fresh.load_connection(connection).is_none());
        assert_eq!(std::fs::read(&path).unwrap(), invalid.as_bytes());
    }
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

#[test]
fn failed_prefetch_flush_preserves_the_newer_pending_memory_snapshot() {
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
    std::os::unix::fs::symlink(&original, &path).unwrap();
    let failing = WorkspaceStore::new(Some(path.clone()), drafts);
    failing.save_connection(connection, state(first, second, "pending first", "pending second"));
    assert!(failing.prefetch_connections(&[connection]).is_err());
    let pending = failing.load_connection(connection).unwrap();
    assert!(matches!(&pending.tabs[0], WorkspaceTabRecord::Editor { query, .. } if query == "pending first"));
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(&original, &path).unwrap();
    assert_eq!(flush(&failing), Ok(()));
    failing.prefetch_connections(&[connection]).unwrap();
    let restored = failing.load_connection(connection).unwrap();
    assert!(matches!(&restored.tabs[0], WorkspaceTabRecord::Editor { query, .. } if query == "pending first"));
}
