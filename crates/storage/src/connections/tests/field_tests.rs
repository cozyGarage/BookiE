use super::*;

#[tokio::test]
async fn clearing_an_optional_field_is_not_undone_by_field_preservation() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut connection = sample_connection();
    connection.tls_root_cert = Some(PathBuf::from("/etc/tablepro/corp-ca.crt"));
    save_to(&path, std::slice::from_ref(&connection)).await.unwrap();

    connection.tls_root_cert = None;
    save_to(&path, std::slice::from_ref(&connection)).await.unwrap();

    let rewritten: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(rewritten["connections"][0].get("tls_root_cert").is_none());
    assert!(load_from(&path).await.unwrap()[0].tls_root_cert.is_none());
}

#[tokio::test]
async fn clearing_a_saved_timeout_is_not_undone_by_field_preservation() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut connection = sample_connection();
    connection.connect_timeout_secs = Some(5);
    connection.query_timeout_secs = Some(30);
    save_to(&path, std::slice::from_ref(&connection)).await.unwrap();

    connection.connect_timeout_secs = None;
    connection.query_timeout_secs = None;
    save_to(&path, std::slice::from_ref(&connection)).await.unwrap();

    let rewritten: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(rewritten["connections"][0].get("connect_timeout_secs").is_none());
    assert!(rewritten["connections"][0].get("query_timeout_secs").is_none());
    let loaded = load_from(&path).await.unwrap();
    assert_eq!(loaded[0].connect_timeout_secs, None);
    assert_eq!(loaded[0].query_timeout_secs, None);
}

#[tokio::test]
async fn the_known_field_list_covers_every_serialized_key() {
    let mut connection = sample_connection();
    connection.socket_dir = Some(PathBuf::from("/run/postgresql"));
    connection.socket_path = Some(PathBuf::from("/run/mysqld/mysqld.sock"));
    connection.tls_root_cert = Some(PathBuf::from("/etc/ca.crt"));
    connection.last_opened_at = Some(Utc::now());
    connection.connect_timeout_secs = Some(5);
    connection.query_timeout_secs = Some(30);
    connection.auth_source = Some("admin".into());
    connection.attached_databases = vec![tablepro_core::AttachedDatabase::new("aux", "/tmp/aux.db")];
    connection.ssh = Some(SavedSshConfig {
        hop_id: Uuid::new_v4(),
        credential_revision: 0,
        host: "bastion".into(),
        port: 22,
        username: "u".into(),
        auth: SavedSshAuth::Password,
        jump: None,
        client: Default::default(),
        agent: false,
    });
    let value = serde_json::to_value(&connection).unwrap();
    let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
    for key in &keys {
        assert!(
            KNOWN_CONNECTION_FIELDS.contains(&key.as_str()),
            "{key} is serialized but missing from KNOWN_CONNECTION_FIELDS"
        );
    }
    for known in KNOWN_CONNECTION_FIELDS {
        assert!(
            keys.iter().any(|key| key.as_str() == *known),
            "{known} is listed but no longer serialized"
        );
    }
}
