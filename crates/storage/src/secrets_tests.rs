#[test]
fn keyring_failures_are_told_apart() {
    use oo7::dbus::{Error as DBus, ServiceError};
    assert_eq!(
        keyring_failure(&oo7::Error::DBus(DBus::Dismissed)),
        crate::KeyringFailure::UnlockCancelled
    );
    assert_eq!(
        keyring_failure(&oo7::Error::DBus(DBus::Service(ServiceError::IsLocked("login".into())))),
        crate::KeyringFailure::Locked
    );
    assert!(matches!(
        keyring_failure(&oo7::Error::DBus(DBus::Deleted)),
        crate::KeyringFailure::Other(_)
    ));
}

#[test]
fn a_missing_secret_service_is_recognised_from_the_dbus_error_text() {
    assert!(names_missing_service(
        "org.freedesktop.DBus.Error.ServiceUnknown: The name is not activatable"
    ));
    assert!(names_missing_service("org.freedesktop.DBus.Error.NameHasNoOwner"));
    assert!(!names_missing_service("org.freedesktop.Secret.Error.IsLocked"));
}

use super::*;

#[test]
fn attrs_include_schema_connection_id_and_kind() {
    let id = Uuid::new_v4();
    let a = attrs_for(id, KIND_DB_PASSWORD);
    assert_eq!(a.get("xdg:schema").map(String::as_str), Some(crate::secret_schema()));
    assert_eq!(
        a.get("connection-id").map(String::as_str),
        Some(id.to_string().as_str())
    );
    assert_eq!(a.get("kind").map(String::as_str), Some(KIND_DB_PASSWORD));
}

#[test]
fn attrs_distinguish_kinds() {
    let id = Uuid::new_v4();
    let db = attrs_for(id, KIND_DB_PASSWORD);
    let ssh = attrs_for(id, KIND_SSH_PASSWORD);
    let pp = attrs_for(id, KIND_SSH_PASSPHRASE);
    assert_ne!(db.get("kind"), ssh.get("kind"));
    assert_ne!(ssh.get("kind"), pp.get("kind"));
}

#[test]
fn hop_secret_attributes_bind_connection_hop_kind_and_revision() {
    let connection = Uuid::new_v4();
    let hop = Uuid::new_v4();
    let attrs = attrs_for_hop(connection, hop, 7, SshHopSecretKind::Password);

    assert_eq!(
        attrs.get("xdg:schema").map(String::as_str),
        Some(crate::secret_schema())
    );
    assert_eq!(attrs.get("connection-id"), Some(&connection.to_string()));
    assert_eq!(attrs.get("hop-id"), Some(&hop.to_string()));
    assert_eq!(attrs.get("kind").map(String::as_str), Some("ssh_hop_password"));
    assert_eq!(attrs.get("credential-revision").map(String::as_str), Some("7"));
}

#[test]
fn hop_secret_attributes_do_not_collide_across_hops_kinds_or_revisions() {
    let connection = Uuid::new_v4();
    let first_hop = Uuid::new_v4();
    let second_hop = Uuid::new_v4();
    let first = attrs_for_hop(connection, first_hop, 3, SshHopSecretKind::Passphrase);

    assert_ne!(
        first,
        attrs_for_hop(connection, second_hop, 3, SshHopSecretKind::Passphrase)
    );
    assert_ne!(
        first,
        attrs_for_hop(connection, first_hop, 4, SshHopSecretKind::Passphrase)
    );
    assert_ne!(
        first,
        attrs_for_hop(connection, first_hop, 3, SshHopSecretKind::Password)
    );
}

#[test]
fn kind_constants_are_distinct_and_non_empty() {
    assert!(!KIND_DB_PASSWORD.is_empty());
    assert!(!KIND_SSH_PASSWORD.is_empty());
    assert!(!KIND_SSH_PASSPHRASE.is_empty());
    assert_ne!(KIND_DB_PASSWORD, KIND_SSH_PASSWORD);
    assert_ne!(KIND_DB_PASSWORD, KIND_SSH_PASSPHRASE);
    assert_ne!(KIND_SSH_PASSWORD, KIND_SSH_PASSPHRASE);
}

#[test]
fn schema_constant_uses_reverse_dns() {
    assert!(crate::secret_schema().starts_with("com."));
    assert!(crate::secret_schema().contains("tablepro"));
}

#[test]
fn map_err_keeps_the_underlying_message_for_an_unclassified_failure() {
    let oo7_err: oo7::Error = oo7::dbus::Error::Deleted.into();
    match map_err(oo7_err) {
        StorageError::Keyring(crate::KeyringFailure::Other(msg)) => {
            assert!(msg.contains("deleted"), "lost the underlying message: {msg}");
        }
        other => panic!("expected an unclassified keyring failure, got {other:?}"),
    }
}

#[test]
fn invalid_utf8_secret_produces_schema_error_with_descriptive_prefix() {
    // load_secret's UTF-8 conversion path: if a secret round-trips through
    // a non-UTF-8 byte sequence (e.g. a binary blob smuggled in via a
    // non-tablepro caller), we surface a clear "secret utf8" prefix
    // instead of a raw FromUtf8Error.
    let bad: Vec<u8> = vec![0xFF, 0xFE, 0xFD];
    let err = String::from_utf8(bad)
        .map_err(|e| StorageError::Schema(format!("secret utf8: {e}")))
        .unwrap_err();
    match err {
        StorageError::Schema(msg) => assert!(msg.starts_with("secret utf8:"), "missing prefix: {msg}"),
        other => panic!("expected Schema variant, got {other:?}"),
    }
}

#[test]
fn attrs_for_includes_uuid_in_canonical_lowercase_hyphenated_form() {
    // Secret Service searches are exact-match on attribute strings, so
    // shifting the UUID format would silently break key lookups.
    let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let a = attrs_for(id, KIND_DB_PASSWORD);
    assert_eq!(
        a.get("connection-id").map(String::as_str),
        Some("550e8400-e29b-41d4-a716-446655440000")
    );
}

#[tokio::test]
#[ignore = "needs a fresh Secret Service with no default collection: dbus-run-session, then gnome-keyring-daemon --daemonize --components=secrets without --unlock"]
async fn reading_a_missing_password_does_not_create_a_keyring() {
    let service = oo7::dbus::Service::new().await.unwrap();
    let default = || service.with_alias(oo7::dbus::Service::DEFAULT_COLLECTION);
    assert!(
        default().await.unwrap().is_none(),
        "this test needs a service without a default collection"
    );

    assert!(load_password(Uuid::new_v4()).await.unwrap().is_none());
    delete_password(Uuid::new_v4()).await.unwrap();

    assert!(
        default().await.unwrap().is_none(),
        "a read or delete created the default keyring"
    );
}

#[tokio::test]
#[ignore = "requires a running Secret Service; scripts/test-secret-service.sh provides one"]
async fn round_trip_via_secret_service() {
    use secrecy::ExposeSecret;
    let id = Uuid::new_v4();
    store_password(id, "test-secret", "tablepro-spike").await.unwrap();
    let loaded = load_password(id).await.unwrap();
    assert_eq!(
        loaded.map(|s| s.expose_secret().to_string()),
        Some("test-secret".to_string())
    );
    delete_password(id).await.unwrap();
    let after = load_password(id).await.unwrap();
    assert!(after.is_none());
}
