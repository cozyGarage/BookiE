use super::*;
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

pub(super) fn sample_connection() -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: "Local Postgres".into(),
        driver_id: "postgres".into(),
        host: "localhost".into(),
        port: 5432,
        socket_dir: None,
        socket_path: None,
        database: "postgres".into(),
        username: "postgres".into(),
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

#[tokio::test]
async fn load_returns_empty_when_file_missing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let result = load_from(&path).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn save_then_load_round_trips() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let original = vec![sample_connection()];
    save_to(&path, &original).await.unwrap();
    let loaded = load_from(&path).await.unwrap();
    assert_eq!(original, loaded);
}

#[tokio::test]
async fn an_import_plan_lands_as_one_rewrite() {
    use crate::connection_bundle::{BundleBody, BundleConnection, plan_import};

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let untouched = sample_connection();
    let mut existing = sample_connection();
    existing.name = "Shared".into();
    save_to(&path, &[untouched.clone(), existing.clone()]).await.unwrap();

    let mut incoming = existing.clone();
    incoming.name = "Shared reporting".into();
    let fresh = sample_connection();
    let body = BundleBody {
        connections: vec![
            BundleConnection::from_saved(&incoming),
            BundleConnection::from_saved(&fresh),
        ],
        ..Default::default()
    };
    let plan = plan_import(&load_from(&path).await.unwrap(), &body).unwrap();

    apply_import_to(&path, &plan).await.unwrap();

    let stored = load_from(&path).await.unwrap();
    assert_eq!(stored.len(), 3);
    assert!(stored.iter().any(|c| c.id == untouched.id && c.name == untouched.name));
    assert!(
        stored
            .iter()
            .any(|c| c.id == existing.id && c.name == "Shared reporting")
    );
    assert!(stored.iter().any(|c| c.id == fresh.id));
}

#[tokio::test]
async fn an_import_never_removes_a_connection_the_bundle_omits() {
    use crate::connection_bundle::{BundleBody, plan_import};

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let kept = sample_connection();
    save_to(&path, std::slice::from_ref(&kept)).await.unwrap();

    let plan = plan_import(&load_from(&path).await.unwrap(), &BundleBody::default()).unwrap();
    apply_import_to(&path, &plan).await.unwrap();

    assert_eq!(load_from(&path).await.unwrap(), vec![kept]);
}

#[tokio::test]
async fn rolling_back_an_updated_connection_restores_its_previous_record() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let original = sample_connection();
    save_to(&path, std::slice::from_ref(&original)).await.unwrap();

    let mut changed = original.clone();
    changed.name = "Overwritten".into();
    upsert_to(&path, std::slice::from_ref(&changed)).await.unwrap();
    upsert_to(&path, std::slice::from_ref(&original)).await.unwrap();

    let stored = load_from(&path).await.unwrap();
    assert_eq!(stored, vec![original]);
}

#[tokio::test]
async fn a_duplicate_gets_a_fresh_id_and_a_distinct_name() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let original = sample_connection();
    save_to(&path, std::slice::from_ref(&original)).await.unwrap();

    let copy = duplicate_from(&path, original.id).await.unwrap();

    assert_ne!(copy.id, original.id);
    assert_eq!(copy.name, "Local Postgres (copy)");
    assert_eq!(copy.host, original.host);
    assert_eq!(copy.database, original.database);
    assert_eq!(copy.last_opened_at, None);

    let stored = load_from(&path).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().any(|connection| connection.id == original.id));
    assert!(stored.iter().any(|connection| connection.id == copy.id));
}

#[tokio::test]
async fn a_second_duplicate_does_not_reuse_the_first_copy_name() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let original = sample_connection();
    save_to(&path, std::slice::from_ref(&original)).await.unwrap();

    let first = duplicate_from(&path, original.id).await.unwrap();
    let second = duplicate_from(&path, original.id).await.unwrap();

    assert_eq!(first.name, "Local Postgres (copy)");
    assert_eq!(second.name, "Local Postgres (copy 2)");
    assert_ne!(first.id, second.id);
}

#[tokio::test]
async fn a_duplicate_keeps_the_recorded_open_time_off_the_copy() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut original = sample_connection();
    original.last_opened_at = Some(Utc::now());
    save_to(&path, std::slice::from_ref(&original)).await.unwrap();

    let copy = duplicate_from(&path, original.id).await.unwrap();

    assert!(copy.last_opened_at.is_none());
    let stored = load_from(&path).await.unwrap();
    let kept = stored.iter().find(|c| c.id == original.id).unwrap();
    assert_eq!(kept.last_opened_at, original.last_opened_at);
}

#[tokio::test]
async fn duplicating_an_unknown_id_is_an_error_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let original = sample_connection();
    save_to(&path, std::slice::from_ref(&original)).await.unwrap();

    assert!(duplicate_from(&path, Uuid::new_v4()).await.is_err());
    assert_eq!(load_from(&path).await.unwrap().len(), 1);
}

#[tokio::test]
async fn save_creates_parent_directory() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested/dir/connections.json");
    save_to(&path, &[]).await.unwrap();
    assert!(path.exists());
}

#[tokio::test]
async fn a_saved_file_is_readable_only_by_its_owner() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    save_to(&path, &[sample_connection()]).await.unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[tokio::test]
async fn rewriting_an_existing_file_keeps_it_private() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    save_to(&path, &[]).await.unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    save_to(&path, &[sample_connection()]).await.unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[tokio::test]
async fn a_shared_temporary_path_does_not_block_a_save() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let shared_tmp = path.with_extension("json.tmp");
    std::fs::create_dir(&shared_tmp).unwrap();
    let original = vec![sample_connection()];

    save_to(&path, &original).await.unwrap();

    assert_eq!(load_from(&path).await.unwrap(), original);
}

#[tokio::test]
async fn a_truncated_file_is_refused_instead_of_loading_as_empty() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    save_to(&path, &[sample_connection()]).await.unwrap();
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
    assert!(load_from(&path).await.is_err());
}

#[tokio::test]
async fn load_rejects_a_file_over_the_size_limit() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(u64::try_from(MAX_CONNECTIONS_FILE_BYTES + 1).unwrap())
        .unwrap();

    let error = load_from(&path).await.unwrap_err();

    assert!(matches!(
        error,
        StorageError::TooLarge {
            limit: MAX_CONNECTIONS_FILE_BYTES,
            ..
        }
    ));
}

#[tokio::test]
async fn load_rejects_excessive_connection_entries() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let file = ConnectionsFile {
        version: CURRENT_VERSION,
        connections: vec![sample_connection(); MAX_CONNECTIONS + 1],
    };
    std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();

    let error = load_from(&path).await.unwrap_err();

    assert!(matches!(error, StorageError::Schema(message) if message.contains("limited")));
}

#[tokio::test]
async fn oversized_serialization_does_not_replace_the_existing_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let prefix = br#"{"version":1,"connections":[],"future":""#;
    let suffix = br#""}"#;
    let filler_len = MAX_CONNECTIONS_FILE_BYTES - prefix.len() - suffix.len() - 1;
    let mut original = Vec::with_capacity(MAX_CONNECTIONS_FILE_BYTES - 1);
    original.extend_from_slice(prefix);
    original.resize(original.len() + filler_len, b'x');
    original.extend_from_slice(suffix);
    std::fs::write(&path, &original).unwrap();

    let error = save_to(&path, &[sample_connection()]).await.unwrap_err();

    assert!(matches!(
        error,
        StorageError::TooLarge {
            limit: MAX_CONNECTIONS_FILE_BYTES,
            ..
        }
    ));
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[tokio::test]
async fn load_rejects_oversized_connection_values() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut connection = sample_connection();
    connection.name = "x".repeat(MAX_CONNECTION_VALUE_BYTES + 1);
    let file = ConnectionsFile {
        version: CURRENT_VERSION,
        connections: vec![connection],
    };
    std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();

    let error = load_from(&path).await.unwrap_err();

    assert!(matches!(
        error,
        StorageError::TooLarge {
            limit: MAX_CONNECTION_VALUE_BYTES,
            ..
        }
    ));
}

#[tokio::test]
async fn local_sqlite_and_duckdb_records_do_not_store_password_fields() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut connections = Vec::new();
    for driver_id in ["sqlite", "duckdb"] {
        let mut connection = sample_connection();
        connection.driver_id = driver_id.into();
        connection.host.clear();
        connection.port = 0;
        connection.database = format!("/tmp/{driver_id}.db");
        connection.username.clear();
        connections.push(connection);
    }

    save_to(&path, &connections).await.unwrap();

    let document: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for record in document["connections"].as_array().unwrap() {
        assert!(record.get("password").is_none());
    }
    assert_eq!(load_from(&path).await.unwrap(), connections);
}

#[tokio::test]
async fn a_certificate_authority_path_round_trips() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut connection = sample_connection();
    connection.tls_mode = Some(TlsMode::VerifyFull);
    connection.tls_root_cert = Some(PathBuf::from("/etc/tablepro/corp-ca.crt"));
    save_to(&path, std::slice::from_ref(&connection)).await.unwrap();
    let loaded = load_from(&path).await.unwrap();
    assert_eq!(loaded[0].tls_root_cert, connection.tls_root_cert);
}

#[tokio::test]
async fn client_tls_identity_paths_round_trip_without_storing_key_material() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut connection = sample_connection();
    connection.tls_mode = Some(TlsMode::VerifyFull);
    connection.tls_client_cert = Some(PathBuf::from("/etc/bookie/client.crt"));
    connection.tls_client_key = Some(PathBuf::from("/etc/bookie/client.key"));

    save_to(&path, std::slice::from_ref(&connection)).await.unwrap();

    let stored = std::fs::read_to_string(&path).unwrap();
    assert!(!stored.contains("PRIVATE KEY"));
    assert_eq!(load_from(&path).await.unwrap(), vec![connection]);
}

#[tokio::test]
async fn a_client_certificate_without_its_key_is_rejected_from_storage() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut connection = sample_connection();
    connection.tls_client_cert = Some(PathBuf::from("/etc/bookie/client.crt"));

    let error = save_to(&path, &[connection]).await.unwrap_err();

    assert!(matches!(error, StorageError::Schema(_)));
}

#[test]
fn the_agent_flag_is_written_only_when_set_so_older_builds_still_read_the_file() {
    let mut ssh = SavedSshConfig {
        hop_id: Uuid::new_v4(),
        credential_revision: 0,
        host: "bastion".into(),
        port: 22,
        username: "deploy".into(),
        auth: SavedSshAuth::PrivateKey {
            path: PathBuf::from("/home/me/.ssh/id_ed25519"),
            has_passphrase: false,
        },
        jump: None,
        client: SshClient::Builtin,
        agent: false,
        agent_identity: None,
    };
    assert!(!serde_json::to_string(&ssh).unwrap().contains("agent"));
    ssh.agent = true;
    let text = serde_json::to_string(&ssh).unwrap();
    assert!(text.contains("\"agent\":true"), "{text}");
    assert!(serde_json::from_str::<SavedSshConfig>(&text).unwrap().agent);
}

#[tokio::test]
async fn a_file_without_a_certificate_authority_still_loads() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let id = Uuid::new_v4();
    let legacy = format!(
        r#"{{"version":1,"connections":[{{
            "id":"{id}","name":"Old","driver_id":"postgres",
            "host":"db.example","port":5432,"database":"postgres",
            "username":"postgres","use_tls":true}}]}}"#
    );
    tokio::fs::write(&path, legacy).await.unwrap();
    let loaded = load_from(&path).await.unwrap();
    assert!(loaded[0].tls_root_cert.is_none());
    assert_eq!(loaded[0].effective_tls_mode(), TlsMode::VerifyFull);
    assert_eq!(loaded[0].query_timeout_secs, None);
}

#[tokio::test]
async fn load_rejects_unknown_version() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    tokio::fs::write(&path, r#"{"version":999,"connections":[]}"#)
        .await
        .unwrap();
    let err = load_from(&path).await.unwrap_err();
    assert!(matches!(err, StorageError::Schema(_)));
}

#[tokio::test]
async fn load_accepts_legacy_files_without_ssh_field() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let id = Uuid::new_v4();
    let legacy = format!(
        r#"{{"version":1,"connections":[{{
            "id":"{id}","name":"Old","driver_id":"postgres",
            "host":"localhost","port":5432,"database":"postgres",
            "username":"postgres","use_tls":false}}]}}"#
    );
    tokio::fs::write(&path, legacy).await.unwrap();
    let loaded = load_from(&path).await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded[0].socket_dir.is_none());
    assert!(loaded[0].ssh.is_none());
    assert_eq!(loaded[0].environment, Environment::Local);
    assert_eq!(loaded[0].effective_tls_mode(), TlsMode::Disabled);
}

#[tokio::test]
async fn legacy_use_tls_true_maps_to_verify_full() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let id = Uuid::new_v4();
    let legacy = format!(
        r#"{{"version":1,"connections":[{{
            "id":"{id}","name":"Old","driver_id":"postgres",
            "host":"db.example","port":5432,"database":"postgres",
            "username":"postgres","use_tls":true}}]}}"#
    );
    tokio::fs::write(&path, legacy).await.unwrap();
    let loaded = load_from(&path).await.unwrap();
    assert_eq!(loaded[0].effective_tls_mode(), TlsMode::VerifyFull);
}

#[tokio::test]
async fn ssh_config_round_trips() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut conn = sample_connection();
    conn.ssh = Some(SavedSshConfig {
        hop_id: Uuid::new_v4(),
        credential_revision: 0,
        host: "bastion.example.com".into(),
        port: 22,
        username: "deploy".into(),
        auth: SavedSshAuth::PrivateKey {
            path: PathBuf::from("/home/u/.ssh/id_ed25519"),
            has_passphrase: true,
        },
        jump: None,
        client: Default::default(),
        agent: false,
        agent_identity: None,
    });
    save_to(&path, &[conn.clone()]).await.unwrap();
    let loaded = load_from(&path).await.unwrap();
    assert_eq!(loaded, vec![conn]);
}

#[tokio::test]
async fn ssh_jump_chain_round_trips() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut conn = sample_connection();
    conn.ssh = Some(SavedSshConfig {
        hop_id: Uuid::new_v4(),
        credential_revision: 0,
        host: "edge.example.com".into(),
        port: 22,
        username: "edge".into(),
        auth: SavedSshAuth::Password,
        jump: Some(Box::new(SavedSshConfig {
            hop_id: Uuid::new_v4(),
            credential_revision: 0,
            host: "bastion.example.com".into(),
            port: 22,
            username: "deploy".into(),
            auth: SavedSshAuth::PrivateKey {
                path: PathBuf::from("/home/u/.ssh/id_ed25519"),
                has_passphrase: false,
            },
            jump: None,
            client: Default::default(),
            agent: false,
            agent_identity: None,
        })),
        client: Default::default(),
        agent: false,
        agent_identity: None,
    });
    save_to(&path, &[conn.clone()]).await.unwrap();
    let loaded = load_from(&path).await.unwrap();
    assert_eq!(loaded, vec![conn.clone()]);
    let hops = loaded[0].ssh.as_ref().unwrap().flatten_hops();
    assert_eq!(hops.len(), 2);
    assert_eq!(hops[0].host, "edge.example.com");
    assert_eq!(hops[1].host, "bastion.example.com");
}

#[tokio::test]
async fn auth_mode_defaults_to_password_for_legacy_connections() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let id = Uuid::new_v4();
    let legacy = format!(
        r#"{{"version":1,"connections":[{{
            "id":"{id}","name":"Old","driver_id":"mssql",
            "host":"localhost","port":1433,"database":"master",
            "username":"sa","use_tls":false}}]}}"#
    );
    tokio::fs::write(&path, legacy).await.unwrap();

    let loaded = load_from(&path).await.unwrap();

    assert_eq!(loaded[0].auth_mode, AuthMode::Password);
}

#[tokio::test]
async fn kerberos_auth_mode_round_trips() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut connection = sample_connection();
    connection.driver_id = "mssql".into();
    connection.auth_mode = AuthMode::Kerberos;

    save_to(&path, &[connection.clone()]).await.unwrap();
    let bytes = tokio::fs::read(&path).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let loaded = load_from(&path).await.unwrap();

    assert_eq!(json["connections"][0]["auth_mode"], "kerberos");
    assert_eq!(loaded, vec![connection]);
}

#[tokio::test]
async fn load_accepts_legacy_ssh_without_jump() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let id = Uuid::new_v4();
    let legacy = format!(
        r#"{{"version":1,"connections":[{{
            "id":"{id}","name":"Old","driver_id":"postgres",
            "host":"localhost","port":5432,"database":"postgres",
            "username":"postgres","use_tls":false,
            "ssh":{{"host":"bastion","port":22,"username":"u","auth":{{"kind":"password"}}}}
        }}]}}"#
    );
    tokio::fs::write(&path, legacy).await.unwrap();
    let loaded = load_from(&path).await.unwrap();
    assert!(loaded[0].ssh.as_ref().unwrap().jump.is_none());
}

#[tokio::test]
async fn the_known_field_list_covers_every_serialized_key() {
    let mut connection = sample_connection();
    connection.socket_dir = Some(PathBuf::from("/run/postgresql"));
    connection.socket_path = Some(PathBuf::from("/run/mysqld/mysqld.sock"));
    connection.tls_root_cert = Some(PathBuf::from("/etc/ca.crt"));
    connection.last_opened_at = Some(Utc::now());
    connection.auth_source = Some("admin".into());
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
        agent_identity: None,
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

#[tokio::test]
async fn a_field_written_by_a_newer_version_survives_a_rewrite() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let mut connection = sample_connection();
    save_to(&path, std::slice::from_ref(&connection)).await.unwrap();

    let mut document: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    document["connections"][0]["colour_tag"] = serde_json::json!("teal");
    document["future_section"] = serde_json::json!({ "keep": true });
    std::fs::write(&path, serde_json::to_vec_pretty(&document).unwrap()).unwrap();

    connection.name = "Renamed".into();
    save_to(&path, std::slice::from_ref(&connection)).await.unwrap();

    let rewritten: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(rewritten["connections"][0]["colour_tag"], "teal");
    assert_eq!(rewritten["future_section"]["keep"], true);
    assert_eq!(rewritten["connections"][0]["name"], "Renamed");
    assert_eq!(load_from(&path).await.unwrap()[0].name, "Renamed");
}

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
async fn a_file_written_by_this_version_still_loads_unchanged() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let original = vec![sample_connection()];
    save_to(&path, &original).await.unwrap();
    save_to(&path, &original).await.unwrap();
    assert_eq!(load_from(&path).await.unwrap(), original);
}

#[tokio::test]
async fn a_malformed_previous_file_is_preserved_and_blocks_a_save() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let malformed = b"{ not json";
    std::fs::write(&path, malformed).unwrap();

    assert!(save_to(&path, &[sample_connection()]).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), malformed);
}

#[tokio::test]
async fn concurrent_deletions_do_not_restore_another_deleted_connection() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("connections.json");
    let first = sample_connection();
    let second = sample_connection();
    save_to(&path, &[first.clone(), second.clone()]).await.unwrap();
    let first_path = path.clone();
    let second_path = path.clone();

    let (first_result, second_result) =
        tokio::join!(delete_from(&first_path, first.id), delete_from(&second_path, second.id),);

    first_result.unwrap();
    second_result.unwrap();
    assert!(load_from(&path).await.unwrap().is_empty());
}
