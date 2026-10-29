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
