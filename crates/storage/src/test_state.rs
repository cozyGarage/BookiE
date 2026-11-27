use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use secrecy::{ExposeSecret, SecretString};

type Attributes = HashMap<&'static str, String>;

static MEMORY: OnceLock<Mutex<MemorySecrets>> = OnceLock::new();
static STATE_DIR: OnceLock<PathBuf> = OnceLock::new();
static USER_STATE_REQUIRED: AtomicBool = AtomicBool::new(false);

pub fn use_user_state() {
    USER_STATE_REQUIRED.store(true, Ordering::SeqCst);
}

pub fn isolated_test_state_active() -> bool {
    cfg!(feature = "isolated-test-state") && !USER_STATE_REQUIRED.load(Ordering::SeqCst)
}

pub fn isolated_test_state_dir() -> Option<&'static Path> {
    if !isolated_test_state_active() {
        return None;
    }
    let dir = STATE_DIR.get_or_init(|| {
        std::env::temp_dir()
            .join("bookie-test-state")
            .join(format!("{}-{}", std::process::id(), uuid::Uuid::new_v4()))
    });
    Some(dir)
}

pub(crate) fn with_memory<T>(operation: impl FnOnce(&mut MemorySecrets) -> T) -> Option<T> {
    if !isolated_test_state_active() {
        return None;
    }
    let mut secrets = MEMORY
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    Some(operation(&mut secrets))
}

#[derive(Default)]
pub(crate) struct MemorySecrets {
    items: Vec<(Attributes, SecretString)>,
}

impl MemorySecrets {
    pub(crate) fn store(&mut self, attrs: &Attributes, value: &str) {
        self.items.retain(|(stored, _)| stored != attrs);
        self.items
            .push((attrs.clone(), SecretString::new(value.to_owned().into())));
    }

    pub(crate) fn load(&self, attrs: &Attributes) -> Option<SecretString> {
        self.items
            .iter()
            .find(|(stored, _)| matches(stored, attrs))
            .map(|(_, value)| SecretString::new(value.expose_secret().to_owned().into()))
    }

    pub(crate) fn delete(&mut self, attrs: &Attributes) {
        self.items.retain(|(stored, _)| !matches(stored, attrs));
    }
}

fn matches(stored: &Attributes, wanted: &Attributes) -> bool {
    wanted.iter().all(|(name, value)| stored.get(name) == Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(id: &str, kind: &str) -> Attributes {
        HashMap::from([("connection-id", id.to_owned()), ("kind", kind.to_owned())])
    }

    #[test]
    #[cfg(not(feature = "isolated-test-state"))]
    fn a_build_without_the_test_feature_uses_the_secret_service_and_user_directories() {
        assert!(!isolated_test_state_active());
        assert!(isolated_test_state_dir().is_none());
        assert!(with_memory(|memory| memory.load(&attrs("a", "db_password"))).is_none());
    }

    #[test]
    fn a_stored_secret_is_found_replaced_and_deleted_by_its_attributes() {
        let mut secrets = MemorySecrets::default();
        secrets.store(&attrs("a", "db_password"), "first");
        secrets.store(&attrs("a", "db_password"), "second");
        secrets.store(&attrs("b", "db_password"), "other");

        let found = secrets
            .load(&attrs("a", "db_password"))
            .map(|value| value.expose_secret().to_owned());
        assert_eq!(found.as_deref(), Some("second"));
        assert!(secrets.load(&attrs("a", "ssh_password")).is_none());

        secrets.delete(&attrs("a", "db_password"));
        assert!(secrets.load(&attrs("a", "db_password")).is_none());
        assert!(secrets.load(&attrs("b", "db_password")).is_some());
    }

    #[test]
    fn a_search_matches_every_secret_carrying_the_requested_attributes() {
        let mut secrets = MemorySecrets::default();
        let mut hop = attrs("a", "ssh_hop_password");
        hop.insert("hop-id", "h".to_owned());
        secrets.store(&hop, "hop secret");

        assert!(secrets.load(&attrs("a", "ssh_hop_password")).is_some());
        secrets.delete(&attrs("a", "ssh_hop_password"));
        assert!(secrets.load(&hop).is_none());
    }
}
