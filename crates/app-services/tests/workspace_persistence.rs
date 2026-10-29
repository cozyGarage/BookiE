#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_app_services::workspace_state::{ConnectionWorkspaceState, WorkspaceStore, WorkspaceTabRecord};
use uuid::Uuid;

#[test]
fn an_explicit_workspace_path_survives_flush_and_a_fresh_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace_state.json");
    let drafts = dir.path().join("drafts");
    let id = Uuid::new_v4();
    let store = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    store.save_connection(
        id,
        ConnectionWorkspaceState {
            tabs: vec![WorkspaceTabRecord::Editor {
                query: "SELECT 'é';".into(),
                draft_id: None,
                file: Some("/tmp/report.sql".into()),
            }],
            ..Default::default()
        },
    );
    assert_eq!(
        store.flush().recv_timeout(std::time::Duration::from_secs(5)).unwrap(),
        Ok(())
    );
    let restored = WorkspaceStore::new(Some(path.clone()), drafts);
    restored.prefetch_connections(&[id]).unwrap();
    let state = restored.load_connection(id).unwrap();
    assert!(
        matches!(&state.tabs[0], WorkspaceTabRecord::Editor { query, draft_id: Some(_), file: Some(file) }
        if query == "SELECT 'é';" && file == std::path::Path::new("/tmp/report.sql"))
    );
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(json["connections"][id.to_string()]["tabs"][0].get("query").is_none());
}

fn flush(store: &WorkspaceStore) -> Result<(), tablepro_app_services::workspace_state::WorkspaceFlushError> {
    store.flush().recv_timeout(std::time::Duration::from_secs(30)).unwrap()
}

#[test]
fn legacy_records_migrate_without_changing_selection_or_the_disk_contract() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace_state.json");
    let id = Uuid::new_v4();
    let legacy = serde_json::json!({"connections": {id.to_string(): {
        "tabs": [
            {"kind": "future_tab"},
            {"kind": "browse", "schema": "public", "table": "items", "offset": 500,
             "page_size": 500, "sort_col": 2, "sort_asc": false},
            {"kind": "structure", "schema": null, "table": "jobs"}
        ], "active_idx": 1, "collapsed_groups": ["schema:public/views"], "hidden_schemas": ["private"]
    }}});
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let store = WorkspaceStore::new(Some(path.clone()), dir.path().join("drafts"));
    store.prefetch_connections(&[id]).unwrap();
    let state = store.load_connection(id).unwrap();
    assert_eq!(state.active_idx, 0);
    assert_eq!(state.tabs.len(), 2);
    store.save_connection(id, state);
    assert_eq!(flush(&store), Ok(()));
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        saved,
        serde_json::json!({"connections": {id.to_string(): {
            "tabs": [
                {"kind": "table", "schema": "public", "table": "items", "mode": "data", "offset": 500,
                 "page_size": 500, "sort_col": 2, "sort_asc": false},
                {"kind": "table", "schema": null, "table": "jobs", "mode": "structure", "offset": 0,
                 "page_size": 1000, "sort_col": null, "sort_asc": null}
            ], "active_idx": 0, "collapsed_groups": ["schema:public/views"], "hidden_schemas": ["private"]
        }}})
    );
}

#[test]
fn an_unreadable_workspace_is_preserved_and_a_retry_can_flush_the_pending_save() {
    use tablepro_app_services::workspace_state::WorkspaceFlushError;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace_state.json");
    std::fs::write(&path, b"invalid workspace").unwrap();
    let id = Uuid::new_v4();
    let store = WorkspaceStore::new(Some(path.clone()), dir.path().join("drafts"));
    assert_eq!(store.prefetch_connections(&[id]), Err(WorkspaceFlushError::WriteFailed));
    store.save_connection(id, ConnectionWorkspaceState::default());
    assert_eq!(flush(&store), Err(WorkspaceFlushError::WriteFailed));
    assert_eq!(std::fs::read(&path).unwrap(), b"invalid workspace");
    std::fs::write(&path, br#"{"connections":{}}"#).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match store.flush().recv_timeout(remaining) {
            Ok(Ok(())) => break,
            Ok(Err(WorkspaceFlushError::WriteFailed)) if !remaining.is_zero() => {}
            result => panic!("workspace flush did not recover: {result:?}"),
        }
    }
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(saved["connections"][id.to_string()]["tabs"], serde_json::json!([]));
}

#[test]
fn a_missing_config_path_reports_failure_without_creating_drafts() {
    use tablepro_app_services::workspace_state::WorkspaceFlushError;
    let dir = tempfile::tempdir().unwrap();
    let drafts = dir.path().join("drafts");
    let store = WorkspaceStore::new(None, drafts.clone());
    store.save_connection(Uuid::new_v4(), ConnectionWorkspaceState::default());
    assert_eq!(flush(&store), Err(WorkspaceFlushError::WriteFailed));
    assert!(!drafts.exists());
}

#[test]
fn forgetting_a_connection_only_removes_its_own_workspace_and_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace_state.json");
    let drafts = dir.path().join("drafts");
    let store = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    for id in [first, second] {
        store.save_connection(
            id,
            ConnectionWorkspaceState {
                tabs: vec![WorkspaceTabRecord::Editor {
                    query: id.to_string(),
                    draft_id: None,
                    file: None,
                }],
                ..Default::default()
            },
        );
    }
    assert_eq!(flush(&store), Ok(()));
    store.forget_connection(first);
    assert!(!drafts.join(first.to_string()).exists());
    assert!(drafts.join(second.to_string()).is_dir());
    assert!(store.load_connection(first).is_none());
    let restored = WorkspaceStore::new(Some(path), drafts);
    restored.prefetch_connections(&[first, second]).unwrap();
    assert!(restored.load_connection(first).is_none());
    assert!(matches!(&restored.load_connection(second).unwrap().tabs[0],
        WorkspaceTabRecord::Editor { query, .. } if query == &second.to_string()));
}

#[tokio::test]
async fn reimporting_a_deleted_connection_rejects_its_old_session_and_allows_a_new_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let drafts = dir.path().join("drafts");
    let id = Uuid::new_v4();
    let store = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    let old_session = store.connection_save_token(id);
    store.save_connection(
        id,
        ConnectionWorkspaceState {
            tabs: vec![WorkspaceTabRecord::Editor {
                query: "SELECT before_delete".into(),
                draft_id: None,
                file: None,
            }],
            ..Default::default()
        },
    );
    assert_eq!(flush(&store), Ok(()));

    store.forget_connection(id);
    store.allow_connection_saves(id).await.unwrap();
    store.save_database_for_session(
        id,
        "main",
        &old_session,
        ConnectionWorkspaceState {
            tabs: vec![WorkspaceTabRecord::Editor {
                query: "SELECT stale_session".into(),
                draft_id: None,
                file: None,
            }],
            ..Default::default()
        },
    );
    assert_eq!(flush(&store), Ok(()));

    let new_session = store.connection_save_token(id);
    store.save_database_for_session(
        id,
        "main",
        &new_session,
        ConnectionWorkspaceState {
            tabs: vec![WorkspaceTabRecord::Editor {
                query: "SELECT after_reimport".into(),
                draft_id: None,
                file: None,
            }],
            ..Default::default()
        },
    );
    assert_eq!(flush(&store), Ok(()));

    let restored = WorkspaceStore::new(Some(path), drafts);
    restored.prefetch_connections(&[id]).unwrap();
    let state = restored.load_database(id, "main").unwrap();
    assert!(matches!(&state.tabs[0], WorkspaceTabRecord::Editor { query, .. } if query == "SELECT after_reimport"));
}

#[test]
fn startup_preview_cleanup_preserves_the_selected_kept_tab_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace_state.json");
    let id = Uuid::new_v4();
    let store = WorkspaceStore::new(Some(path.clone()), dir.path().join("drafts"));
    let tabs = [true, false]
        .into_iter()
        .map(|preview| WorkspaceTabRecord::Table {
            schema: None,
            table: if preview { "preview" } else { "kept" }.into(),
            mode: Default::default(),
            offset: 0,
            page_size: 1000,
            sort_col: None,
            sort_asc: None,
            preview,
        })
        .collect();
    store.save_connection(
        id,
        ConnectionWorkspaceState {
            tabs,
            active_idx: 1,
            ..Default::default()
        },
    );
    assert_eq!(flush(&store), Ok(()));
    store.discard_preview_tabs();
    let restored = WorkspaceStore::new(Some(path), dir.path().join("drafts"));
    restored.prefetch_connections(&[id]).unwrap();
    let state = restored.load_connection(id).unwrap();
    assert_eq!(state.active_idx, 0);
    assert_eq!(state.tabs.len(), 1);
    assert!(matches!(&state.tabs[0], WorkspaceTabRecord::Table { table, preview: false, .. } if table == "kept"));
}

fn repeated_large_draft_saves(connection_count: usize) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace_state.json");
    let drafts = dir.path().join("drafts");
    let store = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    let ids: Vec<_> = (0..connection_count).map(|_| Uuid::new_v4()).collect();
    let original = "SELECT '東京é';\n".repeat(65_536);
    let record = |query: String| ConnectionWorkspaceState {
        tabs: vec![WorkspaceTabRecord::Editor {
            query,
            draft_id: None,
            file: None,
        }],
        ..Default::default()
    };
    for id in &ids {
        store.save_connection(*id, record(original.clone()));
    }
    assert_eq!(flush(&store), Ok(()));
    let mut latest = original.clone();
    for revision in 0..3 {
        latest = format!("{original}SELECT {revision};\n");
        store.save_connection(ids[0], record(latest.clone()));
        assert_eq!(flush(&store), Ok(()));
    }
    let restored = WorkspaceStore::new(Some(path), drafts);
    restored.prefetch_connections(&ids).unwrap();
    for (index, id) in ids.iter().enumerate() {
        let expected = if index == 0 { &latest } else { &original };
        let state = restored.load_connection(*id).unwrap();
        assert_eq!(state.tabs.len(), 1);
        assert!(matches!(&state.tabs[0], WorkspaceTabRecord::Editor { query, .. } if query == expected));
    }
}

#[test]
fn repeated_large_draft_saves_preserve_other_connections_and_unicode_text() {
    repeated_large_draft_saves(4);
}

#[test]
fn repeated_large_draft_saves_preserve_one_connections_unicode_text() {
    repeated_large_draft_saves(1);
}

#[test]
fn saving_one_connection_preserves_an_external_change_to_an_unrelated_draft() {
    use std::os::unix::fs::MetadataExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace_state.json");
    let drafts = dir.path().join("drafts");
    let store = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    let changed = Uuid::new_v4();
    let untouched = Uuid::new_v4();
    let record = |query: &str| ConnectionWorkspaceState {
        tabs: vec![WorkspaceTabRecord::Editor {
            query: query.into(),
            draft_id: None,
            file: None,
        }],
        ..Default::default()
    };
    store.save_connection(changed, record("SELECT 'changed';"));
    store.save_connection(untouched, record("SELECT 'original';"));
    assert_eq!(flush(&store), Ok(()));

    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let draft_id = json["connections"][untouched.to_string()]["tabs"][0]["draft_id"]
        .as_str()
        .unwrap();
    let untouched_draft = drafts.join(untouched.to_string()).join(format!("{draft_id}.sql"));
    let external_text = "SELECT 'written outside the store';";
    std::fs::write(&untouched_draft, external_text).unwrap();
    let external_inode = std::fs::metadata(&untouched_draft).unwrap().ino();

    store.save_connection(changed, record("SELECT 'changed again';"));
    assert_eq!(flush(&store), Ok(()));

    assert_eq!(std::fs::read_to_string(&untouched_draft).unwrap(), external_text);
    assert_eq!(std::fs::metadata(untouched_draft).unwrap().ino(), external_inode);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        saved["connections"][untouched.to_string()]["tabs"][0]["draft_id"],
        draft_id
    );
}

#[test]
fn an_unreadable_unrequested_draft_does_not_block_a_healthy_workspace() {
    assert_unrelated_workspace_restores(false);
}

#[test]
fn an_unreadable_unrequested_draft_path_does_not_block_a_healthy_workspace() {
    assert_unrelated_workspace_restores(true);
}

fn assert_unrelated_workspace_restores(unreadable_path: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.json");
    let drafts = directory.path().join("drafts");
    let broken = Uuid::new_v4();
    let healthy = Uuid::new_v4();
    let draft = Uuid::new_v4();
    let writer = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    writer.save_connection(
        broken,
        ConnectionWorkspaceState {
            tabs: vec![WorkspaceTabRecord::Editor {
                query: "SELECT 'preserve'".into(),
                draft_id: Some(draft),
                file: None,
            }],
            ..Default::default()
        },
    );
    writer.save_connection(healthy, ConnectionWorkspaceState::default());
    assert_eq!(flush(&writer), Ok(()));
    let body = drafts.join(broken.to_string()).join(format!("{draft}.sql"));
    if unreadable_path {
        std::fs::remove_file(&body).unwrap();
        std::fs::create_dir(&body).unwrap();
    } else {
        std::fs::write(&body, [0xff]).unwrap();
    }
    let metadata = std::fs::read(&path).unwrap();
    let reader = WorkspaceStore::new(Some(path.clone()), drafts);
    assert_eq!(reader.prefetch_connections(&[healthy]), Ok(()));
    assert!(reader.load_connection(healthy).is_some());
    let mixed = WorkspaceStore::new(Some(path.clone()), directory.path().join("drafts"));
    assert!(mixed.prefetch_connections(&[broken, healthy]).is_err());
    assert!(mixed.load_connection(healthy).is_some());
    assert!(mixed.load_connection(broken).is_none());
    assert!(reader.prefetch_connections(&[broken]).is_err());
    if unreadable_path {
        assert!(body.is_dir());
    } else {
        assert_eq!(std::fs::read(&body).unwrap(), [0xff]);
    }
    assert_eq!(std::fs::read(&path).unwrap(), metadata);
    if unreadable_path {
        std::fs::remove_dir(&body).unwrap();
    } else {
        std::fs::remove_file(&body).unwrap();
    }
    assert_eq!(reader.prefetch_connections(&[broken]), Ok(()));
    let restored = reader.load_connection(broken).unwrap();
    assert!(matches!(&restored.tabs[0], WorkspaceTabRecord::Editor { query, draft_id: None, .. } if query.is_empty()));
}

#[test]
fn prefetch_refuses_invalid_connection_metadata_without_caching_requested_workspaces() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.json");
    let id = Uuid::new_v4();
    let bytes = serde_json::to_vec(&serde_json::json!({"connections": {
        "invalid-uuid": {"tabs": []}, id.to_string(): {"tabs": []}
    }}))
    .unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let store = WorkspaceStore::new(Some(path.clone()), directory.path().join("drafts"));
    assert!(store.prefetch_connections(&[id]).is_err());
    assert!(store.load_connection(id).is_none());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

fn two_drafts(first: Uuid, second: Uuid, first_text: &str, second_text: &str) -> ConnectionWorkspaceState {
    ConnectionWorkspaceState {
        tabs: vec![
            WorkspaceTabRecord::Editor {
                query: first_text.into(),
                draft_id: Some(first),
                file: None,
            },
            WorkspaceTabRecord::Editor {
                query: second_text.into(),
                draft_id: Some(second),
                file: None,
            },
        ],
        ..Default::default()
    }
}

#[test]
fn a_failed_later_draft_preserves_the_previous_acknowledged_workspace_body() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.json");
    let drafts = directory.path().join("drafts");
    let id = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let store = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    store.save_connection(id, two_drafts(first, second, "old first", "old second"));
    assert_eq!(flush(&store), Ok(()));
    let metadata = std::fs::read(&path).unwrap();
    let bodies = drafts.join(id.to_string());
    let blocked = bodies.join(format!("{second}.sql"));
    std::fs::remove_file(&blocked).unwrap();
    std::fs::create_dir(&blocked).unwrap();
    store.save_connection(id, two_drafts(first, second, "new first", "new second"));
    assert!(flush(&store).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), metadata);
    assert_eq!(
        std::fs::read_to_string(bodies.join(format!("{first}.sql"))).unwrap(),
        "old first"
    );
    assert_eq!(std::fs::read_dir(&bodies).unwrap().count(), 2);
    std::fs::remove_dir(&blocked).unwrap();
    std::fs::write(&blocked, "old second").unwrap();
    assert_eq!(flush(&store), Ok(()));
    let restored = WorkspaceStore::new(Some(path), drafts);
    restored.prefetch_connections(&[id]).unwrap();
    let saved = restored.load_connection(id).unwrap();
    assert!(matches!(&saved.tabs[0], WorkspaceTabRecord::Editor { query, .. } if query == "new first"));
    assert!(matches!(&saved.tabs[1], WorkspaceTabRecord::Editor { query, .. } if query == "new second"));
    assert_eq!(std::fs::read_dir(&bodies).unwrap().count(), 2);
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
    writer.save_connection(connection, two_drafts(first, second, "old first", "old second"));
    assert_eq!(flush(&writer), Ok(()));
    std::os::unix::fs::symlink(&original, &path).unwrap();
    let failing = WorkspaceStore::new(Some(path.clone()), drafts);
    failing.save_connection(connection, two_drafts(first, second, "pending first", "pending second"));
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
