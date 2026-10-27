#[test]
fn audit_prefetch_does_not_restore_a_deleted_cache_entry() {
    let root = tempfile::tempdir().unwrap();
    let connection = Uuid::new_v4();
    let store = WorkspaceStore::new(Some(root.path().join("workspace.json")), root.path().join("drafts"));
    store.save_connection(connection, ConnectionWorkspaceState::default());
    assert_eq!(store.flush().recv_timeout(std::time::Duration::from_secs(20)).unwrap(), Ok(()));
    prefetch_connections_coordinated(&store.inner.memory, &[connection], || {}, || {
        let snapshot = load_locked(&store.inner.paths)?;
        store.forget_connection(connection);
        Ok(snapshot)
    }).unwrap();
    assert!(store.load_connection(connection).is_none(), "deleted workspace was recached by the late preload");
}
