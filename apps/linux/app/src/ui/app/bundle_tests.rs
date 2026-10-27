use super::*;
use tablepro_core::{AuthMode, Environment, TlsMode};

fn saved(name: &str) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: name.into(),
        driver_id: "postgres".into(),
        host: "localhost".into(),
        port: 5432,
        socket_dir: None,
        socket_path: None,
        database: "app".into(),
        username: "app".into(),
        use_tls: false,
        tls_mode: Some(TlsMode::Disabled),
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: false,
        auth_mode: AuthMode::Password,
        environment: Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
    }
}

fn item(name: &str) -> ImportItem {
    let connection = saved(name);
    ImportItem {
        disposition: ImportDisposition::New,
        source_id: connection.id,
        connection,
        previous: None,
        organization: None,
    }
}

#[test]
fn unticked_entries_are_dropped_and_the_order_survives() {
    let first = item("a");
    let second = item("b");
    let third = item("c");
    let plan = ImportPlan {
        items: vec![first.clone(), second.clone(), third.clone()],
    };

    let kept = accepted_items(&plan, &[third.source_id, first.source_id]);

    assert_eq!(
        kept.iter().map(|i| i.source_id).collect::<Vec<_>>(),
        vec![first.source_id, third.source_id]
    );
}

#[test]
fn ticking_nothing_imports_nothing() {
    let plan = ImportPlan { items: vec![item("a")] };
    assert!(accepted_items(&plan, &[]).is_empty());
}

#[tokio::test]
async fn an_import_failure_stops_before_saving_later_connections() {
    let items = vec![item("first"), item("failed"), item("later")];
    let attempted = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let imported = apply_items(items.clone(), Vec::new(), false, test_workspace(), {
        let attempted = attempted.clone();
        move |item, _, _| {
            let attempted = attempted.clone();
            async move {
                attempted.lock().unwrap().push(item.connection.name.clone());
                if item.connection.name == "failed" {
                    Err(ItemApplyFailure::CredentialStore)
                } else {
                    Ok(())
                }
            }
        }
    })
    .await;

    assert_eq!(imported, (1, Some(ItemApplyFailure::CredentialStore)));
    assert_eq!(*attempted.lock().unwrap(), ["first", "failed"]);
}

#[tokio::test]
async fn an_import_holds_the_save_turn_while_each_connection_is_applied() {
    let held_during_apply = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let outcome = apply_items(vec![item("a"), item("b")], Vec::new(), false, test_workspace(), {
        let held = held_during_apply.clone();
        move |_, _, _| {
            let held = held.clone();
            async move {
                held.lock()
                    .unwrap()
                    .push(crate::services::connection_save_turn::is_held());
                Ok(())
            }
        }
    })
    .await;

    assert_eq!(outcome, (2, None));
    assert_eq!(*held_during_apply.lock().unwrap(), [true, true]);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            crate::services::connection_save_turn::take()
        )
        .await
        .is_ok(),
        "the turn is released afterwards"
    );
}

#[tokio::test]
async fn workspace_save_recovery_failure_is_reported_after_connection_import() {
    let workspace = test_workspace();
    let imported = item("restored");
    let id = imported.connection.id;
    workspace.forget_connection(id);
    workspace.save_connection(Uuid::new_v4(), Default::default());
    assert_eq!(
        workspace.flush().recv().unwrap(),
        Err(tablepro_app_services::workspace_state::WorkspaceFlushError::WriteFailed)
    );

    let outcome = apply_items(vec![imported], Vec::new(), false, workspace, |_, _, _| async { Ok(()) }).await;

    assert_eq!(outcome.0, 1);
    assert_eq!(outcome.1, Some(ItemApplyFailure::WorkspaceSaveRecovery));
}

#[tokio::test]
async fn a_successful_reimport_rejects_old_session_saves_and_allows_new_ones() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.json");
    let drafts = directory.path().join("drafts");
    let workspace = crate::services::workspace_state::WorkspaceStore::new(Some(path.clone()), drafts.clone());
    let imported = item("restored");
    let id = imported.connection.id;
    let old_session = workspace.connection_save_token(id);
    workspace.forget_connection(id);

    let outcome = apply_items(
        vec![imported.clone()],
        Vec::new(),
        false,
        workspace.clone(),
        |_, _, _| async { Ok(()) },
    )
    .await;

    assert_eq!(outcome, (1, None));
    workspace.save_database_for_session(
        id,
        "main",
        &old_session,
        tablepro_app_services::workspace_state::ConnectionWorkspaceState {
            tabs: vec![tablepro_app_services::workspace_state::WorkspaceTabRecord::Editor {
                query: "SELECT stale_session".into(),
                draft_id: None,
                file: None,
            }],
            ..Default::default()
        },
    );
    assert_eq!(workspace.flush().recv().unwrap(), Ok(()));

    let new_session = workspace.connection_save_token(id);
    workspace.save_database_for_session(
        id,
        "main",
        &new_session,
        tablepro_app_services::workspace_state::ConnectionWorkspaceState {
            tabs: vec![tablepro_app_services::workspace_state::WorkspaceTabRecord::Editor {
                query: "SELECT after_reimport".into(),
                draft_id: None,
                file: None,
            }],
            ..Default::default()
        },
    );
    assert_eq!(workspace.flush().recv().unwrap(), Ok(()));
    let reopened = crate::services::workspace_state::WorkspaceStore::new(Some(path), drafts);
    reopened.prefetch_connections(&[id]).unwrap();
    assert!(matches!(
        &reopened.load_database(id, "main").unwrap().tabs[0],
        tablepro_app_services::workspace_state::WorkspaceTabRecord::Editor { query, .. }
            if query == "SELECT after_reimport"
    ));
}

#[test]
fn a_failed_record_restore_keeps_imported_credentials() {
    let mut item = item("new");
    assert!(!should_forget_imported_secrets(&item, false));
    assert!(should_forget_imported_secrets(&item, true));
    item.disposition = ImportDisposition::UpdateInPlace;
    assert!(!should_forget_imported_secrets(&item, true));
}

#[test]
fn a_file_that_is_not_a_bundle_is_reported_rather_than_parsed() {
    assert!(read_bundle_file(b"nonsense").is_err());
}

fn test_workspace() -> crate::services::workspace_state::WorkspaceStore {
    crate::services::workspace_state::WorkspaceStore::new(None, std::path::PathBuf::new())
}

#[test]
fn the_export_file_stamp_is_a_plain_date() {
    let stamp = today_stamp();
    assert_eq!(stamp.len(), 10);
    assert_eq!(stamp.matches('-').count(), 2);
}

#[tokio::test]
async fn a_file_over_the_limit_is_refused_before_it_is_read() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("big.bundle");
    std::fs::File::create(&path).unwrap().set_len(17).unwrap();
    let error = read_bounded(&path, 16).await.unwrap_err();
    assert!(
        matches!(error, BundleError::TooLarge { got: 17, limit: 16 }),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_directory_or_missing_path_is_not_a_bundle() {
    let directory = tempfile::tempdir().unwrap();
    assert!(matches!(
        read_bounded(directory.path(), 16).await,
        Err(BundleError::NotABundle)
    ));
    assert!(matches!(
        read_bounded(&directory.path().join("missing"), 16).await,
        Err(BundleError::NotABundle)
    ));
}

#[tokio::test]
async fn a_file_at_the_limit_is_read_whole() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ok.bundle");
    std::fs::write(&path, [7u8; 16]).unwrap();
    assert_eq!(read_bounded(&path, 16).await.unwrap(), vec![7u8; 16]);
}
