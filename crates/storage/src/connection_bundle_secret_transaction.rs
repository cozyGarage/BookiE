use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use crate::connection_bundle::{BundleSecrets, BundleSshSecretKind};
use crate::error::StorageError;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum SecretKey {
    DatabasePassword,
    SshPassword,
    SshPassphrase,
    SshHopPassword { hop_id: Uuid, credential_revision: u64 },
    SshHopPassphrase { hop_id: Uuid, credential_revision: u64 },
}

trait BundleSecretStore {
    async fn load(&self, target: Uuid, key: &SecretKey) -> Result<Option<SecretString>, StorageError>;
    async fn store(&self, target: Uuid, key: &SecretKey, value: &SecretString, label: &str)
    -> Result<(), StorageError>;
    async fn delete(&self, target: Uuid, key: &SecretKey) -> Result<(), StorageError>;
}

struct Keyring;

impl BundleSecretStore for Keyring {
    async fn load(&self, target: Uuid, key: &SecretKey) -> Result<Option<SecretString>, StorageError> {
        match key {
            SecretKey::DatabasePassword => crate::secrets::load_password(target).await,
            SecretKey::SshPassword => crate::secrets::load_ssh_password(target).await,
            SecretKey::SshPassphrase => crate::secrets::load_ssh_passphrase(target).await,
            SecretKey::SshHopPassword {
                hop_id,
                credential_revision,
            } => crate::secrets::load_ssh_hop_password(target, *hop_id, *credential_revision).await,
            SecretKey::SshHopPassphrase {
                hop_id,
                credential_revision,
            } => crate::secrets::load_ssh_hop_passphrase(target, *hop_id, *credential_revision).await,
        }
    }

    async fn store(
        &self,
        target: Uuid,
        key: &SecretKey,
        value: &SecretString,
        label: &str,
    ) -> Result<(), StorageError> {
        match key {
            SecretKey::DatabasePassword => crate::secrets::store_password(target, value.expose_secret(), label).await,
            SecretKey::SshPassword => crate::secrets::store_ssh_password(target, value.expose_secret(), label).await,
            SecretKey::SshPassphrase => {
                crate::secrets::store_ssh_passphrase(target, value.expose_secret(), label).await
            }
            SecretKey::SshHopPassword {
                hop_id,
                credential_revision,
            } => {
                crate::secrets::store_ssh_hop_password(
                    target,
                    *hop_id,
                    *credential_revision,
                    value.expose_secret(),
                    label,
                )
                .await
            }
            SecretKey::SshHopPassphrase {
                hop_id,
                credential_revision,
            } => {
                crate::secrets::store_ssh_hop_passphrase(
                    target,
                    *hop_id,
                    *credential_revision,
                    value.expose_secret(),
                    label,
                )
                .await
            }
        }
    }

    async fn delete(&self, target: Uuid, key: &SecretKey) -> Result<(), StorageError> {
        match key {
            SecretKey::DatabasePassword => crate::secrets::delete_password(target).await,
            SecretKey::SshPassword => crate::secrets::delete_ssh_password(target).await,
            SecretKey::SshPassphrase => crate::secrets::delete_ssh_passphrase(target).await,
            SecretKey::SshHopPassword {
                hop_id,
                credential_revision,
            } => crate::secrets::delete_ssh_hop_password(target, *hop_id, *credential_revision).await,
            SecretKey::SshHopPassphrase {
                hop_id,
                credential_revision,
            } => crate::secrets::delete_ssh_hop_passphrase(target, *hop_id, *credential_revision).await,
        }
    }
}

pub(super) async fn store_bundle_secrets(
    target: Uuid,
    secrets: &BundleSecrets,
    label: &str,
    replace_existing: bool,
) -> Result<(), StorageError> {
    store_bundle_secrets_with(&Keyring, target, secrets, label, replace_existing).await
}

pub(super) async fn store_import_secrets(
    plan: &crate::connection_bundle::ImportPlan,
    secrets: &[BundleSecrets],
) -> Result<(), StorageError> {
    let imports = plan.items.iter().filter_map(|item| {
        secrets
            .iter()
            .find(|secret| secret.connection_id == item.source_id)
            .map(|secret| (item.connection.id, secret, item.connection.name.as_str()))
    });
    store_import_secrets_with(&Keyring, imports).await
}

async fn store_import_secrets_with<'a, S: BundleSecretStore>(
    store: &S,
    imports: impl IntoIterator<Item = (Uuid, &'a BundleSecrets, &'a str)>,
) -> Result<(), StorageError> {
    let entries = imports
        .into_iter()
        .flat_map(|(target, secrets, label)| {
            entries(secrets)
                .into_iter()
                .map(move |(key, value)| (target, key, value, label))
        })
        .collect::<Vec<_>>();
    let mut previous = Vec::with_capacity(entries.len());
    for (target, key, _, _) in &entries {
        previous.push((*target, key.clone(), store.load(*target, key).await?));
    }

    let mut touched = Vec::new();
    for ((target, key, value, label), (_, _, old_value)) in entries.iter().zip(&previous) {
        if !super::should_write(old_value.clone(), false) {
            continue;
        }
        touched.push((*target, key.clone(), old_value.clone(), (*label).to_owned()));
        if let Err(error) = store.store(*target, key, value, label).await {
            if let Err(restore) = restore_import(store, &touched).await {
                return Err(StorageError::CredentialRollback {
                    store: Box::new(error),
                    restore: Box::new(restore),
                });
            }
            return Err(error);
        }
    }
    Ok(())
}

async fn store_bundle_secrets_with<S: BundleSecretStore>(
    store: &S,
    target: Uuid,
    secrets: &BundleSecrets,
    label: &str,
    replace_existing: bool,
) -> Result<(), StorageError> {
    let entries = entries(secrets);
    let mut previous = Vec::with_capacity(entries.len());
    for (key, _) in &entries {
        previous.push((key.clone(), store.load(target, key).await?));
    }

    let mut touched = Vec::new();
    for ((key, value), (_, old_value)) in entries.iter().zip(&previous) {
        if !super::should_write(old_value.clone(), replace_existing) {
            continue;
        }
        touched.push((key.clone(), old_value.clone()));
        if let Err(error) = store.store(target, key, value, label).await {
            if let Err(restore) = restore(store, target, &touched, label).await {
                return Err(StorageError::CredentialRollback {
                    store: Box::new(error),
                    restore: Box::new(restore),
                });
            }
            return Err(error);
        }
    }
    Ok(())
}

fn entries(secrets: &BundleSecrets) -> Vec<(SecretKey, &SecretString)> {
    let mut entries = Vec::new();
    if let Some(value) = secrets.db_password() {
        entries.push((SecretKey::DatabasePassword, value));
    }
    if let Some(value) = secrets.ssh_password() {
        entries.push((SecretKey::SshPassword, value));
    }
    if let Some(value) = secrets.ssh_passphrase() {
        entries.push((SecretKey::SshPassphrase, value));
    }
    for secret in secrets.ssh_hop_secrets() {
        let key = match secret.kind() {
            BundleSshSecretKind::Password => SecretKey::SshHopPassword {
                hop_id: secret.hop_id(),
                credential_revision: secret.credential_revision(),
            },
            BundleSshSecretKind::Passphrase => SecretKey::SshHopPassphrase {
                hop_id: secret.hop_id(),
                credential_revision: secret.credential_revision(),
            },
        };
        entries.push((key, secret.secret()));
    }
    entries
}

async fn restore<S: BundleSecretStore>(
    store: &S,
    target: Uuid,
    touched: &[(SecretKey, Option<SecretString>)],
    label: &str,
) -> Result<(), StorageError> {
    let mut failure = None;
    for (key, old_value) in touched.iter().rev() {
        let outcome = match old_value {
            Some(value) => store.store(target, key, value, label).await,
            None => store.delete(target, key).await,
        };
        if let Err(error) = outcome
            && failure.is_none()
        {
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

async fn restore_import<S: BundleSecretStore>(
    store: &S,
    touched: &[(Uuid, SecretKey, Option<SecretString>, String)],
) -> Result<(), StorageError> {
    let mut failure = None;
    for (target, key, old_value, label) in touched.iter().rev() {
        let outcome = match old_value {
            Some(value) => store.store(*target, key, value, label).await,
            None => store.delete(*target, key).await,
        };
        if let Err(error) = outcome
            && failure.is_none()
        {
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use secrecy::{ExposeSecret, SecretString};
    use uuid::Uuid;

    use super::*;
    use crate::connection_bundle::{BundleSshSecret, BundleSshSecretKind};

    #[derive(Default)]
    struct FakeStore {
        values: Mutex<HashMap<SecretKey, SecretString>>,
        write_calls: AtomicUsize,
        fail_calls: Mutex<std::collections::HashSet<usize>>,
        fail_delete: Mutex<bool>,
    }

    impl FakeStore {
        fn seed(&self, key: SecretKey, value: &str) {
            self.values
                .lock()
                .unwrap()
                .insert(key, SecretString::new(value.to_owned().into()));
        }

        fn value(&self, key: &SecretKey) -> Option<String> {
            self.values
                .lock()
                .unwrap()
                .get(key)
                .map(|value| value.expose_secret().to_owned())
        }
    }

    impl BundleSecretStore for FakeStore {
        async fn load(
            &self,
            _target: Uuid,
            key: &SecretKey,
        ) -> Result<Option<SecretString>, crate::error::StorageError> {
            Ok(self.values.lock().unwrap().get(key).cloned())
        }

        async fn store(
            &self,
            _target: Uuid,
            key: &SecretKey,
            value: &SecretString,
            _label: &str,
        ) -> Result<(), crate::error::StorageError> {
            let call = self.write_calls.fetch_add(1, Ordering::SeqCst);
            if call == 1 || self.fail_calls.lock().unwrap().contains(&call) {
                return Err(crate::error::StorageError::Keyring(crate::KeyringFailure::Unavailable));
            }
            self.values.lock().unwrap().insert(key.clone(), value.clone());
            Ok(())
        }

        async fn delete(&self, _target: Uuid, key: &SecretKey) -> Result<(), crate::error::StorageError> {
            if *self.fail_delete.lock().unwrap() {
                return Err(crate::error::StorageError::Keyring(crate::KeyringFailure::Unavailable));
            }
            self.values.lock().unwrap().remove(key);
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_failed_single_bundle_credential_rollback_is_reported() {
        let target = Uuid::new_v4();
        let hop_id = Uuid::new_v4();
        let store = FakeStore::default();
        store.seed(SecretKey::DatabasePassword, "old");
        store.seed(
            SecretKey::SshHopPassword {
                hop_id,
                credential_revision: 0,
            },
            "old-hop",
        );
        store.fail_calls.lock().unwrap().insert(2);
        let mut carried = BundleSecrets::new(target, Some(SecretString::new("new".into())), None, None);
        carried.set_ssh_hop_secrets(vec![BundleSshSecret::new(
            hop_id,
            0,
            BundleSshSecretKind::Password,
            SecretString::new("new-hop".into()),
        )]);

        assert!(matches!(
            store_bundle_secrets_with(&store, target, &carried, "bundle-test", true).await,
            Err(StorageError::CredentialRollback { .. })
        ));
    }

    #[tokio::test]
    async fn a_later_secret_write_failure_restores_values_replaced_earlier() {
        let target = Uuid::new_v4();
        let hop_id = Uuid::new_v4();
        let database_key = SecretKey::DatabasePassword;
        let hop_key = SecretKey::SshHopPassword {
            hop_id,
            credential_revision: 7,
        };
        let store = FakeStore::default();
        store.seed(database_key.clone(), "old-database-secret");
        store.seed(hop_key.clone(), "old-hop-secret");

        let mut carried = BundleSecrets::new(
            Uuid::new_v4(),
            Some(SecretString::new("new-database-secret".to_owned().into())),
            None,
            None,
        );
        carried.set_ssh_hop_secrets(vec![BundleSshSecret::new(
            hop_id,
            7,
            BundleSshSecretKind::Password,
            SecretString::new("new-hop-secret".to_owned().into()),
        )]);

        let result = store_bundle_secrets_with(&store, target, &carried, "bundle-test", true).await;

        assert!(result.is_err());
        assert_eq!(store.value(&database_key).as_deref(), Some("old-database-secret"));
        assert_eq!(store.value(&hop_key).as_deref(), Some("old-hop-secret"));
    }

    #[tokio::test]
    async fn a_later_connection_secret_failure_restores_earlier_imported_credentials() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let first_secret = BundleSecrets::new(
            Uuid::new_v4(),
            Some(SecretString::new("first-secret".to_owned().into())),
            None,
            None,
        );
        let second_secret = BundleSecrets::new(
            Uuid::new_v4(),
            Some(SecretString::new("second-secret".to_owned().into())),
            None,
            None,
        );
        let store = FakeStore::default();

        let result = store_import_secrets_with(
            &store,
            [(first, &first_secret, "first"), (second, &second_secret, "second")],
        )
        .await;

        assert!(result.is_err());
        assert_eq!(store.value(&SecretKey::DatabasePassword), None);
    }

    #[tokio::test]
    async fn a_failed_credential_rollback_is_reported() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let first_secret = BundleSecrets::new(
            Uuid::new_v4(),
            Some(SecretString::new("first-secret".to_owned().into())),
            None,
            None,
        );
        let second_secret = BundleSecrets::new(
            Uuid::new_v4(),
            Some(SecretString::new("second-secret".to_owned().into())),
            None,
            None,
        );
        let store = FakeStore::default();
        *store.fail_delete.lock().unwrap() = true;

        let result = store_import_secrets_with(
            &store,
            [(first, &first_secret, "first"), (second, &second_secret, "second")],
        )
        .await;

        assert!(matches!(result, Err(StorageError::CredentialRollback { .. })));
    }
}
