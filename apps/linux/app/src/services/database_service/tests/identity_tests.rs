use super::*;

#[tokio::test]
async fn session_identity_survives_new_guards_but_changes_on_reconnect() {
    let service = DatabaseService::new();
    let id = open_memory_connection(&service, "identity").await;
    let (first_guard, first) = service.get_with_identity(id).unwrap();
    let (second_guard, second) = service.get_with_identity(id).unwrap();
    assert!(!Arc::ptr_eq(&first_guard, &second_guard));
    assert_eq!(first, second);
    let origin = crate::services::catalog::CatalogOrigin {
        id,
        identity: first.clone(),
    };
    assert!(origin.guard(&service).is_some());
    assert!(origin.matches(Some(id), Some(&second)));
    assert!(!origin.matches(Some(Uuid::new_v4()), Some(&second)));
    let mut index = tablepro_app_services::completion::SchemaIndex::default();
    assert!(index.sync_connection(&first));
    index.set_columns("items", vec!["id".into()]);
    let request = index.request("items".into());
    assert!(!index.sync_connection(&second));
    assert!(index.knows_columns("items"));
    assert!(index.accepts(&request));
    let replacement = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    {
        let entries = service.connections.lock().unwrap();
        entries.get(&id).unwrap().inner.lock().unwrap().connection = Arc::from(replacement);
    }
    let current = service.identity(id).unwrap();
    assert_ne!(first, current);
    assert!(origin.guard(&service).is_none());
    assert!(!origin.matches(Some(id), Some(&current)));
    assert!(index.sync_connection(&current));
    assert!(!index.accepts(&request));
    service.close(id);
    assert!(service.identity(id).is_none());
    assert!(!origin.matches(Some(id), None));
}
