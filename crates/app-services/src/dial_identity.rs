use std::hash::{DefaultHasher, Hash, Hasher};

use bookie_storage::SavedConnection;
use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialSetting {
    Host,
    Port,
    Database,
    Username,
    Password,
    Socket,
    Encryption,
    Authentication,
    SshTunnel,
    ReadOnly,
    Environment,
    QueryTimeout,
    ConnectTimeout,
    AuthSource,
    AttachedDatabases,
}

impl DialSetting {
    pub fn label(self) -> String {
        match self {
            Self::Host => crate::tr!("host"),
            Self::Port => crate::tr!("port"),
            Self::Database => crate::tr!("database"),
            Self::Username => crate::tr!("user name"),
            Self::Password => crate::tr!("password"),
            Self::Socket => crate::tr!("socket"),
            Self::Encryption => crate::tr!("encryption"),
            Self::Authentication => crate::tr!("authentication method"),
            Self::SshTunnel => crate::tr!("SSH tunnel"),
            Self::ReadOnly => crate::tr!("read-only mode"),
            Self::Environment => crate::tr!("environment"),
            Self::QueryTimeout => crate::tr!("query timeout"),
            Self::ConnectTimeout => crate::tr!("connect timeout"),
            Self::AuthSource => crate::tr!("authentication source"),
            Self::AttachedDatabases => crate::tr!("attached databases"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialIdentity {
    record: SavedConnection,
    password_digest: u64,
}

impl DialIdentity {
    pub fn new(saved: &SavedConnection, password: &SecretString) -> Self {
        let mut record = saved.clone();
        record.name = String::new();
        record.last_opened_at = None;
        let mut hasher = DefaultHasher::new();
        password.expose_secret().hash(&mut hasher);
        Self {
            record,
            password_digest: hasher.finish(),
        }
    }

    pub fn changed_settings(&self, next: &Self) -> Vec<DialSetting> {
        let (a, b) = (&self.record, &next.record);
        let checks = [
            (DialSetting::Host, a.host != b.host),
            (DialSetting::Port, a.port != b.port),
            (DialSetting::Database, a.database != b.database),
            (DialSetting::Username, a.username != b.username),
            (DialSetting::Password, self.password_digest != next.password_digest),
            (
                DialSetting::Socket,
                (&a.socket_dir, &a.socket_path) != (&b.socket_dir, &b.socket_path),
            ),
            (
                DialSetting::Encryption,
                (
                    a.effective_tls_mode(),
                    &a.tls_root_cert,
                    &a.tls_client_cert,
                    &a.tls_client_key,
                    a.tls_strict_encryption,
                ) != (
                    b.effective_tls_mode(),
                    &b.tls_root_cert,
                    &b.tls_client_cert,
                    &b.tls_client_key,
                    b.tls_strict_encryption,
                ),
            ),
            (DialSetting::Authentication, a.auth_mode != b.auth_mode),
            (DialSetting::SshTunnel, a.ssh != b.ssh),
            (DialSetting::ReadOnly, a.read_only != b.read_only),
            (DialSetting::Environment, a.environment != b.environment),
            (DialSetting::QueryTimeout, a.query_timeout_secs != b.query_timeout_secs),
            (
                DialSetting::ConnectTimeout,
                a.connect_timeout_secs != b.connect_timeout_secs,
            ),
            (DialSetting::AuthSource, a.auth_source != b.auth_source),
            (
                DialSetting::AttachedDatabases,
                a.attached_databases != b.attached_databases,
            ),
        ];
        checks
            .into_iter()
            .filter_map(|(setting, changed)| changed.then_some(setting))
            .collect()
    }
}

pub fn reconnect_notice(changes: &[DialSetting]) -> Option<String> {
    if changes.is_empty() {
        return None;
    }
    let names = changes
        .iter()
        .map(|setting| setting.label())
        .collect::<Vec<_>>()
        .join(", ");
    Some(crate::tr!(
        "Saved. The open session still uses the previous {settings}. Disconnect and connect again to apply the change."
    )
    .replace("{settings}", &names))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::Environment;

    fn saved() -> SavedConnection {
        SavedConnection {
            id: uuid::Uuid::nil(),
            name: "primary".into(),
            driver_id: "postgres".into(),
            host: "db1".into(),
            port: 5432,
            socket_dir: None,
            socket_path: None,
            database: "app".into(),
            username: "ada".into(),
            use_tls: false,
            tls_mode: None,
            tls_root_cert: None,
            tls_client_cert: None,
            tls_client_key: None,
            tls_strict_encryption: false,
            read_only: false,
            auth_mode: Default::default(),
            environment: Environment::default(),
            ssh: None,
            last_opened_at: None,
            connect_timeout_secs: None,
            query_timeout_secs: None,
            auth_source: None,
            attached_databases: Vec::new(),
        }
    }

    fn identity(saved: &SavedConnection, password: &str) -> DialIdentity {
        DialIdentity::new(saved, &SecretString::from(password.to_string()))
    }

    #[test]
    fn cosmetic_edits_and_recency_are_not_dial_changes() {
        let live = identity(&saved(), "pw");
        let mut next = saved();
        next.name = "renamed".into();
        next.last_opened_at = Some(chrono::Utc::now());
        assert!(live.changed_settings(&identity(&next, "pw")).is_empty());
        assert_eq!(reconnect_notice(&[]), None);
    }

    #[test]
    fn endpoint_credential_and_safety_edits_each_name_their_setting() {
        let live = identity(&saved(), "pw");
        let mut next = saved();
        next.host = "db2".into();
        next.port = 6432;
        next.database = "other".into();
        next.username = "grace".into();
        next.read_only = true;
        next.environment = Environment::Prod;
        next.query_timeout_secs = Some(5);
        assert_eq!(
            live.changed_settings(&identity(&next, "pw")),
            vec![
                DialSetting::Host,
                DialSetting::Port,
                DialSetting::Database,
                DialSetting::Username,
                DialSetting::ReadOnly,
                DialSetting::Environment,
                DialSetting::QueryTimeout,
            ]
        );
    }

    #[test]
    fn a_new_password_is_a_dial_change_without_keeping_the_old_one() {
        let live = identity(&saved(), "old");
        assert_eq!(
            live.changed_settings(&identity(&saved(), "new")),
            vec![DialSetting::Password]
        );
        assert!(!format!("{live:?}").contains("old"));
    }

    #[test]
    fn encryption_edits_count_through_the_legacy_flag_and_the_mode() {
        let live = identity(&saved(), "pw");
        let mut next = saved();
        next.tls_mode = Some(bookie_core::TlsMode::Require);
        assert_eq!(
            live.changed_settings(&identity(&next, "pw")),
            vec![DialSetting::Encryption]
        );
    }

    #[test]
    fn the_notice_lists_every_changed_setting() {
        let text = reconnect_notice(&[DialSetting::Host, DialSetting::Password]).unwrap();
        assert!(text.contains("host, password"));
    }
}
