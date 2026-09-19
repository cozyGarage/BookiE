use std::collections::HashSet;
use std::path::PathBuf;

use secrecy::{ExposeSecret, SecretString};

use super::*;
use crate::connections::{KNOWN_CONNECTION_FIELDS, SavedSshAuth};

const PASSPHRASE: &str = "correct horse battery staple";

fn saved(name: &str) -> SavedConnection {
    SavedConnection {
        id: Uuid::new_v4(),
        name: name.into(),
        driver_id: "postgres".into(),
        host: "db.example".into(),
        port: 5432,
        socket_dir: None,
        database: "sales".into(),
        username: "reader".into(),
        use_tls: true,
        tls_mode: Some(TlsMode::Require),
        tls_root_cert: None,
        read_only: false,
        auth_mode: AuthMode::Password,
        environment: Environment::Dev,
        ssh: None,
        last_opened_at: Some(chrono::Utc::now()),
    }
}

fn export_of(connections: Vec<SavedConnection>, secrets: Vec<BundleSecrets>) -> BundleExport {
    BundleExport {
        producer: "bookie 0.2.0".into(),
        exported_at: "2026-09-19T10:00:00Z".into(),
        connections,
        organization: Vec::new(),
        secrets,
    }
}

fn secrets_for(id: Uuid) -> BundleSecrets {
    BundleSecrets::new(
        id,
        Some(SecretString::new("hunter2".to_owned().into())),
        Some(SecretString::new("bastion-pw".to_owned().into())),
        None,
    )
}

fn plaintext_body(bytes: &[u8]) -> BundleBody {
    match parse_bundle(bytes).expect("parse") {
        ParsedBundle::Plaintext(body) => body,
        ParsedBundle::Encrypted(_) => panic!("expected a plaintext bundle"),
    }
}

fn encrypted_body(bytes: &[u8], passphrase: &str) -> Result<BundleBody, BundleError> {
    match parse_bundle(bytes).expect("parse") {
        ParsedBundle::Encrypted(sealed) => sealed.unlock(passphrase),
        ParsedBundle::Plaintext(_) => panic!("expected an encrypted bundle"),
    }
}

#[test]
fn the_bundle_classifies_every_saved_connection_field() {
    let known: HashSet<&str> = KNOWN_CONNECTION_FIELDS.iter().copied().collect();
    let included: HashSet<&str> = BUNDLE_INCLUDED_FIELDS.iter().copied().collect();
    let excluded: HashSet<&str> = BUNDLE_EXCLUDED_FIELDS.iter().copied().collect();

    assert!(
        included.is_disjoint(&excluded),
        "a field is both exported and withheld: {:?}",
        included.intersection(&excluded).collect::<Vec<_>>()
    );
    let classified: HashSet<&str> = included.union(&excluded).copied().collect();
    assert_eq!(
        classified, known,
        "every field of a saved connection must be classified as exported or withheld"
    );
    assert_eq!(included.len() + excluded.len(), KNOWN_CONNECTION_FIELDS.len());
}

#[test]
fn the_exported_record_carries_exactly_the_included_fields() {
    let connection = saved("sales");
    let bytes = export_plaintext(&export_of(vec![connection], Vec::new())).expect("export");
    let document: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    let record = document["payload"]["body"]["connections"][0]
        .as_object()
        .expect("connection object");

    for field in BUNDLE_EXCLUDED_FIELDS {
        assert!(!record.contains_key(*field), "{field} must not be exported");
    }
    for field in BUNDLE_INCLUDED_FIELDS {
        let optional = matches!(*field, "socket_dir" | "tls_root_cert" | "ssh");
        assert!(
            optional || record.contains_key(*field),
            "{field} is missing from the exported record"
        );
    }
}

#[test]
fn a_legacy_use_tls_flag_cannot_contradict_the_exported_tls_mode() {
    let mut connection = saved("legacy");
    connection.use_tls = true;
    connection.tls_mode = None;
    let bytes = export_plaintext(&export_of(vec![connection], Vec::new())).expect("export");
    let body = plaintext_body(&bytes);
    assert_eq!(body.connections[0].tls_mode, TlsMode::VerifyFull);
    assert!(!String::from_utf8_lossy(&bytes).contains("use_tls"));
}

#[test]
fn a_bundle_without_a_passphrase_carries_no_credentials() {
    let connection = saved("sales");
    let id = connection.id;
    let bytes = export_plaintext(&export_of(vec![connection], vec![secrets_for(id)])).expect("export");
    let rendered = String::from_utf8_lossy(&bytes);
    assert!(!rendered.contains("hunter2"));
    assert!(!rendered.contains("bastion-pw"));
    assert!(plaintext_body(&bytes).secrets.is_empty());
}

#[test]
fn a_plaintext_payload_carrying_credentials_is_rejected() {
    let connection = saved("sales");
    let id = connection.id;
    let mut document: serde_json::Value =
        serde_json::from_slice(&export_plaintext(&export_of(vec![connection], Vec::new())).expect("export"))
            .expect("json");
    document["payload"]["body"]["secrets"] = serde_json::json!([{
        "connection_id": id,
        "db_password": "hunter2",
        "ssh_password": null,
        "ssh_passphrase": null,
    }]);
    let bytes = serde_json::to_vec(&document).expect("json");

    assert!(matches!(parse_bundle(&bytes), Err(BundleError::PlaintextSecrets)));
}

#[test]
fn no_bundle_shape_has_a_field_for_an_mcp_token() {
    let connection = saved("sales");
    let id = connection.id;
    let plain = export_plaintext(&export_of(vec![connection.clone()], Vec::new())).expect("export");
    let sealed = export_encrypted(&export_of(vec![connection], vec![secrets_for(id)]), PASSPHRASE).expect("export");

    assert!(!String::from_utf8_lossy(&plain).contains("mcp_token"));
    assert!(!String::from_utf8_lossy(&sealed).contains("mcp_token"));
    let body = encrypted_body(&sealed, PASSPHRASE).expect("unlock");
    let rendered = serde_json::to_string(&body.secrets).expect("json");
    assert!(!rendered.contains("mcp_token"));
}

#[test]
fn an_encrypted_bundle_round_trips_its_credentials() {
    let connection = saved("sales");
    let id = connection.id;
    let bytes = export_encrypted(&export_of(vec![connection], vec![secrets_for(id)]), PASSPHRASE).expect("export");
    let rendered = String::from_utf8_lossy(&bytes);
    assert!(!rendered.contains("hunter2"));
    assert!(!rendered.contains("db.example"));

    let body = encrypted_body(&bytes, PASSPHRASE).expect("unlock");
    assert_eq!(body.connections.len(), 1);
    let secrets = &body.secrets[0];
    assert_eq!(secrets.connection_id, id);
    assert_eq!(secrets.db_password().map(|v| v.expose_secret()), Some("hunter2"));
    assert_eq!(secrets.ssh_password().map(|v| v.expose_secret()), Some("bastion-pw"));
    assert!(secrets.ssh_passphrase().is_none());
}

#[test]
fn a_wrong_passphrase_and_a_changed_file_report_the_same_failure() {
    let connection = saved("sales");
    let id = connection.id;
    let bytes = export_encrypted(&export_of(vec![connection], vec![secrets_for(id)]), PASSPHRASE).expect("export");
    let wrong = encrypted_body(&bytes, "not the passphrase").unwrap_err();

    let mut document: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    let ciphertext = document["payload"]["ciphertext"]
        .as_str()
        .expect("ciphertext")
        .to_owned();
    let mut raw = BASE64.decode(&ciphertext).expect("base64");
    raw[0] ^= 0x01;
    document["payload"]["ciphertext"] = serde_json::Value::String(BASE64.encode(&raw));
    let tampered = serde_json::to_vec(&document).expect("json");
    let changed = encrypted_body(&tampered, PASSPHRASE).unwrap_err();

    assert!(matches!(wrong, BundleError::DecryptionFailed));
    assert!(matches!(changed, BundleError::DecryptionFailed));
    assert_eq!(wrong.to_string(), changed.to_string());
}

#[test]
fn a_memory_bomb_kdf_header_is_refused_at_parse_time() {
    let connection = saved("sales");
    let bytes = export_encrypted(&export_of(vec![connection], Vec::new()), PASSPHRASE).expect("export");
    let mut document: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    document["payload"]["kdf"]["memory_kib"] = serde_json::json!(u32::MAX);
    let bomb = serde_json::to_vec(&document).expect("json");

    assert!(matches!(parse_bundle(&bomb), Err(BundleError::KdfParameters)));
}

#[test]
fn a_document_that_is_not_a_bundle_is_refused() {
    assert!(matches!(parse_bundle(b"{}"), Err(BundleError::Malformed)));
    assert!(matches!(parse_bundle(b"not json"), Err(BundleError::Malformed)));
    let alien = serde_json::json!({
        "format": "something.else",
        "version": 1,
        "producer": "x",
        "exported_at": "y",
        "payload": {"kind": "plaintext", "body": {"connections": []}},
    });
    assert!(matches!(
        parse_bundle(&serde_json::to_vec(&alien).expect("json")),
        Err(BundleError::NotABundle)
    ));
}

#[test]
fn a_future_bundle_version_is_refused_rather_than_guessed() {
    let bytes = export_plaintext(&export_of(vec![saved("sales")], Vec::new())).expect("export");
    let mut document: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    document["version"] = serde_json::json!(2);
    assert!(matches!(
        parse_bundle(&serde_json::to_vec(&document).expect("json")),
        Err(BundleError::UnsupportedVersion(2))
    ));
}

fn ssh_chain(depth: usize) -> SavedSshConfig {
    let mut hop = SavedSshConfig {
        host: "bastion".into(),
        port: 22,
        username: "jump".into(),
        auth: SavedSshAuth::Password,
        jump: None,
    };
    for _ in 1..depth {
        hop = SavedSshConfig {
            host: "bastion".into(),
            port: 22,
            username: "jump".into(),
            auth: SavedSshAuth::Password,
            jump: Some(Box::new(hop)),
        };
    }
    hop
}

#[test]
fn an_ssh_chain_deeper_than_the_cap_is_refused_on_import() {
    let mut connection = saved("deep");
    connection.ssh = Some(ssh_chain(MAX_SSH_HOPS + 1));
    let body = BundleBody {
        connections: vec![BundleConnection::from_saved(&connection)],
        ..Default::default()
    };

    assert!(matches!(
        plan_import(&[], &body),
        Err(BundleError::SshChainTooDeep { .. })
    ));

    let mut shallow = saved("fine");
    shallow.ssh = Some(ssh_chain(MAX_SSH_HOPS));
    let ok = BundleBody {
        connections: vec![BundleConnection::from_saved(&shallow)],
        ..Default::default()
    };
    assert!(plan_import(&[], &ok).is_ok());
}

fn body_of(connections: &[SavedConnection]) -> BundleBody {
    BundleBody {
        connections: connections.iter().map(BundleConnection::from_saved).collect(),
        ..Default::default()
    }
}

#[test]
fn a_fresh_id_is_imported_unchanged() {
    let incoming = saved("sales");
    let plan = plan_import(&[], &body_of(std::slice::from_ref(&incoming))).expect("plan");

    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].disposition, ImportDisposition::New);
    assert_eq!(plan.items[0].connection.id, incoming.id);
    assert_eq!(plan.items[0].source_id, incoming.id);
    assert!(plan.items[0].previous.is_none());
}

#[test]
fn the_same_id_on_the_same_endpoint_updates_in_place() {
    let local = saved("sales");
    let mut incoming = local.clone();
    incoming.name = "Sales reporting".into();
    let plan = plan_import(std::slice::from_ref(&local), &body_of(&[incoming])).expect("plan");

    assert_eq!(plan.items[0].disposition, ImportDisposition::UpdateInPlace);
    assert_eq!(plan.items[0].connection.id, local.id);
    assert_eq!(plan.items[0].connection.name, "Sales reporting");
    assert_eq!(plan.items[0].previous.as_ref().map(|p| p.id), Some(local.id));
}

#[test]
fn the_same_id_on_a_different_endpoint_never_overwrites_the_local_record() {
    let local = saved("sales");
    let mut incoming = local.clone();
    incoming.host = "other.example".into();
    let plan = plan_import(std::slice::from_ref(&local), &body_of(&[incoming])).expect("plan");

    assert_eq!(plan.items[0].disposition, ImportDisposition::Remapped);
    assert_ne!(plan.items[0].connection.id, local.id);
    assert_eq!(plan.items[0].source_id, local.id);
    assert_eq!(plan.items[0].connection.host, "other.example");
    assert!(plan.items[0].previous.is_none());
}

#[test]
fn an_endpoint_match_ignores_host_case_and_settings() {
    let local = saved("sales");
    let mut incoming = local.clone();
    incoming.host = local.host.to_uppercase();
    incoming.name = "renamed".into();
    incoming.auth_mode = AuthMode::Kerberos;
    let plan = plan_import(std::slice::from_ref(&local), &body_of(&[incoming])).expect("plan");

    assert_eq!(plan.items[0].disposition, ImportDisposition::UpdateInPlace);
    assert_eq!(plan.items[0].connection.auth_mode, AuthMode::Kerberos);
}

#[test]
fn a_different_ssh_hop_chain_is_a_different_endpoint() {
    let local = saved("sales");
    let mut incoming = local.clone();
    incoming.ssh = Some(ssh_chain(1));
    let plan = plan_import(std::slice::from_ref(&local), &body_of(&[incoming])).expect("plan");

    assert_eq!(plan.items[0].disposition, ImportDisposition::Remapped);
}

#[test]
fn a_bundle_listing_one_id_twice_is_rejected_whole() {
    let connection = saved("sales");
    let body = body_of(&[connection.clone(), connection]);

    assert!(matches!(
        plan_import(&[], &body),
        Err(BundleError::DuplicateConnectionId)
    ));
}

#[test]
fn a_local_connection_absent_from_the_bundle_is_left_alone() {
    let kept = saved("kept");
    let incoming = saved("incoming");
    let plan = plan_import(std::slice::from_ref(&kept), &body_of(std::slice::from_ref(&incoming))).expect("plan");

    assert_eq!(plan.items.len(), 1);
    assert!(plan.items.iter().all(|item| item.connection.id != kept.id));
}

#[test]
fn an_in_place_update_cannot_clear_read_only() {
    let mut local = saved("sales");
    local.read_only = true;
    let mut incoming = local.clone();
    incoming.read_only = false;
    let plan = plan_import(std::slice::from_ref(&local), &body_of(&[incoming])).expect("plan");

    assert!(plan.items[0].connection.read_only);
}

#[test]
fn an_in_place_update_cannot_lower_the_environment() {
    let mut local = saved("sales");
    local.environment = Environment::Prod;
    let mut incoming = local.clone();
    incoming.environment = Environment::Local;
    let plan = plan_import(std::slice::from_ref(&local), &body_of(&[incoming])).expect("plan");

    assert_eq!(plan.items[0].connection.environment, Environment::Prod);
}

#[test]
fn an_in_place_update_can_raise_the_environment() {
    let mut local = saved("sales");
    local.environment = Environment::Local;
    let mut incoming = local.clone();
    incoming.environment = Environment::Staging;
    let plan = plan_import(std::slice::from_ref(&local), &body_of(&[incoming])).expect("plan");

    assert_eq!(plan.items[0].connection.environment, Environment::Staging);
}

#[test]
fn an_in_place_update_cannot_weaken_tls() {
    let mut local = saved("sales");
    local.tls_mode = Some(TlsMode::VerifyFull);
    let mut incoming = local.clone();
    incoming.tls_mode = Some(TlsMode::Disabled);
    let plan = plan_import(std::slice::from_ref(&local), &body_of(&[incoming])).expect("plan");

    assert_eq!(plan.items[0].connection.tls_mode, Some(TlsMode::VerifyFull));
    assert!(plan.items[0].connection.use_tls);
}

#[test]
fn an_in_place_update_can_strengthen_tls() {
    let mut local = saved("sales");
    local.tls_mode = Some(TlsMode::Require);
    let mut incoming = local.clone();
    incoming.tls_mode = Some(TlsMode::VerifyFull);
    let plan = plan_import(std::slice::from_ref(&local), &body_of(&[incoming])).expect("plan");

    assert_eq!(plan.items[0].connection.tls_mode, Some(TlsMode::VerifyFull));
}

#[test]
fn an_in_place_update_keeps_the_local_last_opened_stamp() {
    let local = saved("sales");
    let incoming = local.clone();
    let plan = plan_import(std::slice::from_ref(&local), &body_of(&[incoming])).expect("plan");

    assert_eq!(plan.items[0].connection.last_opened_at, local.last_opened_at);
}

#[test]
fn an_imported_new_connection_carries_no_last_opened_stamp() {
    let incoming = saved("sales");
    let plan = plan_import(&[], &body_of(&[incoming])).expect("plan");

    assert!(plan.items[0].connection.last_opened_at.is_none());
}

#[test]
fn the_organization_entry_travels_with_its_connection() {
    let connection = saved("sales");
    let entry = ConnectionOrganization::new(Some("Billing"), &["audit".into()], true).expect("organization");
    let export = BundleExport {
        organization: vec![(connection.id, entry)],
        ..export_of(vec![connection.clone()], Vec::new())
    };
    let bytes = export_plaintext(&export).expect("export");
    let body = plaintext_body(&bytes);

    let organization = body.organization.get(&connection.id).expect("entry");
    assert_eq!(organization.group.as_deref(), Some("Billing"));
    assert_eq!(organization.tags, vec!["audit".to_owned()]);

    let plan = plan_import(&[], &body).expect("plan");
    assert_eq!(
        plan.items[0].organization.as_ref().and_then(|o| o.group.as_deref()),
        Some("Billing")
    );
}

#[test]
fn credentials_never_reach_a_debug_line() {
    let id = Uuid::new_v4();
    let rendered = format!("{:?}", secrets_for(id));

    assert!(rendered.contains(&id.to_string()));
    assert!(!rendered.contains("hunter2"));
    assert!(!rendered.contains("bastion-pw"));
}

#[test]
fn no_error_message_carries_a_secret_a_host_or_a_database_name() {
    let id = Uuid::new_v4();
    let variants = [
        BundleError::NotABundle,
        BundleError::UnsupportedVersion(9),
        BundleError::Malformed,
        BundleError::Field("payload.kind"),
        BundleError::PlaintextSecrets,
        BundleError::TooLarge {
            got: 1,
            limit: MAX_BUNDLE_BYTES,
        },
        BundleError::TooManyConnections {
            got: 1,
            limit: MAX_BUNDLE_CONNECTIONS,
        },
        BundleError::SshChainTooDeep {
            got: 9,
            limit: MAX_SSH_HOPS,
        },
        BundleError::DuplicateConnectionId,
        BundleError::KdfParameters,
        BundleError::DecryptionFailed,
        BundleError::EncryptionFailed,
        BundleError::EmptyPassphrase,
        BundleError::SecretUnavailable(id),
    ];

    for variant in variants {
        let message = variant.to_string();
        assert!(!message.is_empty());
        for forbidden in ["hunter2", "bastion-pw", "db.example", "sales", "reader", PASSPHRASE] {
            assert!(!message.contains(forbidden), "{message} leaks {forbidden}");
        }
    }
}

#[test]
fn a_bundle_over_the_size_limit_is_refused_before_parsing() {
    let oversized = vec![b'{'; MAX_BUNDLE_BYTES + 1];
    assert!(matches!(parse_bundle(&oversized), Err(BundleError::TooLarge { .. })));
}

#[test]
fn a_socket_connection_keeps_its_directory_through_a_bundle() {
    let mut connection = saved("local");
    connection.socket_dir = Some(PathBuf::from("/run/postgresql"));
    connection.host = String::new();
    let bytes = export_plaintext(&export_of(vec![connection.clone()], Vec::new())).expect("export");
    let body = plaintext_body(&bytes);

    assert_eq!(
        body.connections[0].socket_dir.as_deref(),
        connection.socket_dir.as_deref()
    );
    let plan = plan_import(std::slice::from_ref(&connection), &body).expect("plan");
    assert_eq!(plan.items[0].disposition, ImportDisposition::UpdateInPlace);
}

#[test]
fn an_import_keeps_an_existing_local_password_unless_replacement_is_asked_for() {
    let existing = || Some(SecretString::new("local".to_owned().into()));

    assert!(!should_write(existing(), false));
    assert!(should_write(existing(), true));
    assert!(should_write(None, false));
    assert!(should_write(None, true));
}

#[tokio::test]
#[ignore = "requires a running Secret Service; scripts/test-secret-service.sh provides one"]
async fn an_export_collects_every_credential_kind_for_a_connection() {
    let id = Uuid::new_v4();
    crate::secrets::store_password(id, "db-pw", "bundle-test")
        .await
        .unwrap();
    crate::secrets::store_ssh_password(id, "ssh-pw", "bundle-test")
        .await
        .unwrap();

    let collected = collect_bundle_secrets(std::slice::from_ref(&id)).await.unwrap();

    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].connection_id, id);
    assert_eq!(collected[0].db_password().map(|v| v.expose_secret()), Some("db-pw"));
    assert_eq!(collected[0].ssh_password().map(|v| v.expose_secret()), Some("ssh-pw"));
    assert!(collected[0].ssh_passphrase().is_none());

    forget_imported_secrets(id).await;
}

#[tokio::test]
#[ignore = "requires a running Secret Service; scripts/test-secret-service.sh provides one"]
async fn a_connection_with_no_stored_credentials_contributes_nothing_to_a_bundle() {
    let id = Uuid::new_v4();
    assert!(
        collect_bundle_secrets(std::slice::from_ref(&id))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "requires a running Secret Service; scripts/test-secret-service.sh provides one"]
async fn an_import_keeps_the_local_password_until_replacement_is_asked_for() {
    let target = Uuid::new_v4();
    crate::secrets::store_password(target, "local-pw", "bundle-test")
        .await
        .unwrap();
    let carried = BundleSecrets::new(
        Uuid::new_v4(),
        Some(SecretString::new("bundled-pw".to_owned().into())),
        None,
        None,
    );

    store_bundle_secrets(target, &carried, "bundle-test", false)
        .await
        .unwrap();
    let kept = crate::secrets::load_password(target).await.unwrap();
    assert_eq!(kept.map(|v| v.expose_secret().to_owned()), Some("local-pw".to_owned()));

    store_bundle_secrets(target, &carried, "bundle-test", true)
        .await
        .unwrap();
    let replaced = crate::secrets::load_password(target).await.unwrap();
    assert_eq!(
        replaced.map(|v| v.expose_secret().to_owned()),
        Some("bundled-pw".to_owned())
    );

    forget_imported_secrets(target).await;
}

#[tokio::test]
#[ignore = "requires a running Secret Service; scripts/test-secret-service.sh provides one"]
async fn rolling_back_an_import_removes_only_the_credentials_it_wrote() {
    let target = Uuid::new_v4();
    let carried = BundleSecrets::new(
        target,
        Some(SecretString::new("bundled-pw".to_owned().into())),
        Some(SecretString::new("bundled-ssh".to_owned().into())),
        None,
    );
    store_bundle_secrets(target, &carried, "bundle-test", false)
        .await
        .unwrap();

    forget_imported_secrets(target).await;

    assert!(crate::secrets::load_password(target).await.unwrap().is_none());
    assert!(crate::secrets::load_ssh_password(target).await.unwrap().is_none());
}
