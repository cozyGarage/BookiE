use bookie_core::{AuthMode, Environment, TlsMode};
use std::path::PathBuf;

use bookie_storage::{
    SavedConnection, SavedSshAuth, SavedSshConfig, delete_connection, delete_password, save_connections, store_password,
};

use crate::connection_form::LocalSocketPathKind;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

pub struct SshDraft {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key_path: String,
    pub secret: SecretString,
}

pub struct ConnectionDraft {
    pub id: Option<Uuid>,
    pub name: String,
    pub driver_id: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: SecretString,
    pub auth_mode: AuthMode,
    pub tls_mode: TlsMode,
    pub tls_root_cert: String,
    pub tls_client_cert: String,
    pub tls_client_key: String,
    pub socket_kind: Option<LocalSocketPathKind>,
    pub socket_endpoint: String,
    pub ssh: Option<SshDraft>,
    pub read_only: bool,
    pub environment: Environment,
    pub keep_stored_password: bool,
    pub keep_saved_ssh: bool,
}

fn path_text(path: Option<&std::path::PathBuf>) -> String {
    path.map(|path| path.display().to_string()).unwrap_or_default()
}

fn optional_path(text: &str) -> Option<PathBuf> {
    let text = text.trim();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

pub async fn load_draft(id: Uuid) -> Result<Option<ConnectionDraft>, String> {
    let saved = bookie_storage::load_connections()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|saved| saved.id == id);
    Ok(saved.map(|saved| draft_from_saved(&saved)))
}

fn draft_from_saved(saved: &SavedConnection) -> ConnectionDraft {
    let single_hop = saved
        .ssh
        .as_ref()
        .filter(|ssh| ssh.jump.is_none() && !ssh.agent && ssh.client == bookie_storage::SshClient::Builtin);
    let (socket_kind, socket_endpoint) = match (&saved.socket_dir, &saved.socket_path) {
        (Some(directory), _) => (Some(LocalSocketPathKind::Directory), path_text(Some(directory))),
        (None, Some(path)) => (Some(LocalSocketPathKind::File), path_text(Some(path))),
        (None, None) => (None, String::new()),
    };
    ConnectionDraft {
        id: Some(saved.id),
        name: saved.name.clone(),
        driver_id: saved.driver_id.clone(),
        host: saved.host.clone(),
        port: saved.port,
        database: saved.database.clone(),
        username: saved.username.clone(),
        password: SecretString::from(String::new()),
        auth_mode: saved.auth_mode,
        tls_mode: saved.effective_tls_mode(),
        tls_root_cert: path_text(saved.tls_root_cert.as_ref()),
        tls_client_cert: path_text(saved.tls_client_cert.as_ref()),
        tls_client_key: path_text(saved.tls_client_key.as_ref()),
        socket_kind,
        socket_endpoint,
        ssh: single_hop.map(|ssh| SshDraft {
            host: ssh.host.clone(),
            port: ssh.port,
            username: ssh.username.clone(),
            key_path: match &ssh.auth {
                bookie_storage::SavedSshAuth::PrivateKey { path, .. } => path.display().to_string(),
                bookie_storage::SavedSshAuth::Password => String::new(),
            },
            secret: SecretString::from(String::new()),
        }),
        read_only: saved.read_only,
        environment: saved.environment,
        keep_stored_password: true,
        keep_saved_ssh: saved.ssh.is_some() && single_hop.is_none(),
    }
}

pub fn derived_name(driver_id: &str, host: &str, username: &str, database: &str) -> String {
    if driver_id == "sqlite" || driver_id == "duckdb" {
        return database.to_owned();
    }
    format!("{username}@{host}")
}

pub fn validate(draft: &ConnectionDraft) -> Result<(), String> {
    let file_based = matches!(draft.driver_id.as_str(), "sqlite" | "duckdb");
    if draft.driver_id.trim().is_empty() {
        return Err("Choose a database type.".into());
    }
    if file_based {
        return if draft.database.trim().is_empty() {
            Err("Enter the database file.".into())
        } else {
            Ok(())
        };
    }
    let socket = draft.socket_endpoint.trim();
    if socket.is_empty() && draft.host.trim().is_empty() {
        return Err("Enter the host.".into());
    }
    if !socket.is_empty() && !crate::connection_form::socket_path_is_absolute(socket) {
        return Err("The socket path must be absolute.".into());
    }
    if draft.port == 0 && socket.is_empty() {
        return Err("Enter a port from 1 to 65535.".into());
    }
    if draft.tls_client_cert.trim().is_empty() != draft.tls_client_key.trim().is_empty() {
        return Err("A client certificate needs its key, and a key needs its certificate.".into());
    }
    validate_ssh(draft.ssh.as_ref())
}

fn validate_ssh(ssh: Option<&SshDraft>) -> Result<(), String> {
    let Some(ssh) = ssh else {
        return Ok(());
    };
    if ssh.host.trim().is_empty() || ssh.username.trim().is_empty() {
        return Err("Enter the SSH host and user.".into());
    }
    if ssh.port == 0 {
        return Err("Enter an SSH port from 1 to 65535.".into());
    }
    Ok(())
}

pub fn saved_from_draft(draft: &ConnectionDraft, id: Uuid) -> SavedConnection {
    let name = match draft.name.trim() {
        "" => derived_name(
            &draft.driver_id,
            draft.host.trim(),
            draft.username.trim(),
            draft.database.trim(),
        ),
        entered => entered.to_owned(),
    };
    let (socket_dir, socket_path) =
        crate::connection_form::local_socket_options(draft.socket_kind, draft.socket_endpoint.trim());
    SavedConnection {
        id,
        name,
        driver_id: draft.driver_id.clone(),
        host: draft.host.trim().to_owned(),
        port: draft.port,
        socket_dir,
        socket_path,
        database: draft.database.trim().to_owned(),
        username: draft.username.trim().to_owned(),
        use_tls: draft.tls_mode.encrypts(),
        tls_mode: Some(draft.tls_mode),
        tls_root_cert: optional_path(&draft.tls_root_cert),
        tls_client_cert: optional_path(&draft.tls_client_cert),
        tls_client_key: optional_path(&draft.tls_client_key),
        tls_strict_encryption: false,
        auth_mode: draft.auth_mode,
        read_only: draft.read_only,
        environment: draft.environment,
        ssh: draft.ssh.as_ref().map(|ssh| ssh_config(ssh)),
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        auth_source: None,
        attached_databases: Vec::new(),
    }
}

fn ssh_config(ssh: &SshDraft) -> SavedSshConfig {
    SavedSshConfig {
        hop_id: Uuid::new_v4(),
        credential_revision: 0,
        host: ssh.host.trim().to_owned(),
        port: ssh.port,
        username: ssh.username.trim().to_owned(),
        auth: match optional_path(&ssh.key_path) {
            Some(path) => SavedSshAuth::PrivateKey {
                path,
                has_passphrase: !ssh.secret.expose_secret().is_empty(),
            },
            None => SavedSshAuth::Password,
        },
        jump: None,
        client: bookie_storage::SshClient::Builtin,
        agent: false,
        agent_identity: None,
        local_port: None,
    }
}

pub async fn save(draft: ConnectionDraft) -> Result<SavedConnection, String> {
    validate(&draft)?;
    let _turn = crate::connection_save_turn::take().await;
    let existing = match draft.id {
        Some(id) => bookie_storage::load_connections()
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|saved| saved.id == id),
        None => None,
    };
    let mut saved = saved_from_draft(&draft, draft.id.unwrap_or_else(Uuid::new_v4));
    if let Some(previous) = &existing {
        carry_over(&mut saved, previous, &draft);
    }
    let keep_password = draft.keep_stored_password && draft.password.expose_secret().is_empty() && existing.is_some();
    if keep_password {
        save_connections(std::slice::from_ref(&saved))
            .await
            .map_err(|error| format!("save: {error}"))?;
    } else {
        persist_with_password(&saved, existing.as_ref(), &draft.password, &saved.name).await?;
    }
    store_ssh_secret(&saved, draft.ssh.as_ref()).await?;
    Ok(saved)
}

fn carry_over(saved: &mut SavedConnection, previous: &SavedConnection, draft: &ConnectionDraft) {
    if draft.keep_saved_ssh {
        saved.ssh = previous.ssh.clone();
    } else if let (Some(new), Some(old)) = (saved.ssh.as_mut(), previous.ssh.as_ref()) {
        new.hop_id = old.hop_id;
        new.credential_revision = old.credential_revision;
    }
    saved.auth_source = previous.auth_source.clone();
    saved.connect_timeout_secs = previous.connect_timeout_secs;
    saved.query_timeout_secs = previous.query_timeout_secs;
    saved.attached_databases = previous.attached_databases.clone();
    saved.tls_strict_encryption = previous.tls_strict_encryption;
    saved.last_opened_at = previous.last_opened_at;
}

async fn store_ssh_secret(saved: &SavedConnection, ssh: Option<&SshDraft>) -> Result<(), String> {
    let (Some(config), Some(draft)) = (&saved.ssh, ssh) else {
        return Ok(());
    };
    let secret = draft.secret.expose_secret();
    if secret.is_empty() {
        return Ok(());
    }
    let label = &saved.name;
    match &config.auth {
        SavedSshAuth::Password => bookie_storage::store_ssh_password(saved.id, secret, label)
            .await
            .map_err(|error| format!("Connection settings were saved, but the SSH password was not saved: {error}. Re-enter the SSH password and connect again.")),
        SavedSshAuth::PrivateKey { .. } => bookie_storage::store_ssh_passphrase(saved.id, secret, label)
            .await
            .map_err(|error| format!("Connection settings were saved, but the SSH key passphrase was not saved: {error}. Re-enter the passphrase and connect again.")),
    }
}

pub struct Deleted {
    pub credential_failure: bool,
}

pub async fn delete(id: Uuid) -> Result<Deleted, String> {
    let _turn = crate::connection_save_turn::take().await;
    let chain = bookie_storage::load_connections()
        .await
        .ok()
        .and_then(|all| all.into_iter().find(|saved| saved.id == id))
        .and_then(|saved| saved.ssh);
    bookie_storage::delete_connection(id)
        .await
        .map_err(|error| format!("The connection could not be deleted: {error}"))?;
    let mut credential_failure = false;
    let results = [
        bookie_storage::delete_password(id).await.err(),
        bookie_storage::delete_ssh_password(id).await.err(),
        bookie_storage::delete_ssh_passphrase(id).await.err(),
    ];
    for error in results.into_iter().flatten() {
        credential_failure = true;
        tracing::warn!(%error, "deleting connection credentials failed");
    }
    if let Some(chain) = &chain
        && let Err(error) = crate::ssh_chain_secrets::delete_chain_secrets(id, chain).await
    {
        credential_failure = true;
        tracing::warn!(%error, "deleting jump host credentials failed");
    }
    Ok(Deleted { credential_failure })
}

pub async fn test(
    registry: &bookie_core::DriverRegistry,
    draft: &ConnectionDraft,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<String, String> {
    validate(draft)?;
    if draft.ssh.is_some() && draft.id.is_none() {
        return Err("Save the connection first to test it through SSH.".into());
    }
    let driver = registry
        .get(&draft.driver_id)
        .ok_or_else(|| format!("driver {} is not available", draft.driver_id))?;
    let saved = saved_from_draft(draft, draft.id.unwrap_or_else(Uuid::new_v4));
    let hops = bookie_transport::saved_ssh_route(&saved)
        .await
        .map_err(|error| error.to_string())?;
    let mut opts = bookie_transport::connect_options_for(&saved)
        .await
        .map_err(|error| error.to_string())?;
    if !draft.password.expose_secret().is_empty() {
        opts.password = SecretString::from(draft.password.expose_secret().to_owned());
    }
    opts.application_name = Some("BookiE".into());
    let environment = bookie_transport::SshEnvironment::builtin(bookie_ssh::UnknownHostKey::Refuse);
    let (connection, _tunnel) =
        bookie_transport::establish_with_cancellation(&*driver, opts, hops, &environment, cancellation.clone())
            .await
            .map_err(|error| error.to_string())?;
    let control = crate::operation_control::bounded_with(15, cancellation);
    match connection.server_version_controlled(&control).await {
        Ok(Some(version)) => Ok(format!("Connected · {version}")),
        _ => Ok("Connected".into()),
    }
}

pub async fn persist_with_password(
    saved: &SavedConnection,
    existing: Option<&SavedConnection>,
    password: &SecretString,
    label: &str,
) -> Result<(), String> {
    save_connections(std::slice::from_ref(saved))
        .await
        .map_err(|error| format!("save: {error}"))?;
    match saved.auth_mode {
        AuthMode::Password if password.expose_secret().is_empty() => {
            if let Err(error) = delete_password(saved.id).await {
                return Err(rollback_failure(existing, saved.id, "delete obsolete stored password", error).await);
            }
        }
        AuthMode::Password => {
            if let Err(error) = store_password(saved.id, password.expose_secret(), label).await {
                return Err(rollback_failure(existing, saved.id, "store password", error).await);
            }
        }
        AuthMode::Kerberos if existing.is_some_and(|connection| connection.auth_mode == AuthMode::Password) => {
            if let Err(error) = delete_password(saved.id).await {
                return Err(rollback_failure(existing, saved.id, "delete stored password", error).await);
            }
        }
        AuthMode::Kerberos => {}
    }
    Ok(())
}

async fn rollback_failure(
    existing: Option<&SavedConnection>,
    id: Uuid,
    action: &str,
    error: bookie_storage::StorageError,
) -> String {
    match rollback_saved_connection(existing, id).await {
        Ok(()) => format!("{action}: {error}"),
        Err(rollback_error) => format!("{action}: {error}; restore saved connection: {rollback_error}"),
    }
}

async fn rollback_saved_connection(
    previous: Option<&SavedConnection>,
    id: Uuid,
) -> Result<(), bookie_storage::StorageError> {
    match previous {
        Some(connection) => save_connections(std::slice::from_ref(connection)).await,
        None => delete_connection(id).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> ConnectionDraft {
        ConnectionDraft {
            id: None,
            name: String::new(),
            driver_id: "postgres".into(),
            host: " db.example ".into(),
            port: 5432,
            database: "app".into(),
            username: "ada".into(),
            password: SecretString::from("secret".to_owned()),
            auth_mode: AuthMode::Password,
            tls_mode: TlsMode::VerifyFull,
            tls_root_cert: String::new(),
            tls_client_cert: String::new(),
            tls_client_key: String::new(),
            socket_kind: None,
            socket_endpoint: String::new(),
            ssh: None,
            read_only: true,
            environment: Environment::Local,
            keep_stored_password: false,
            keep_saved_ssh: false,
        }
    }

    #[test]
    fn an_empty_name_is_derived_and_fields_are_trimmed() {
        let saved = saved_from_draft(&draft(), Uuid::nil());
        assert_eq!(
            (saved.name.as_str(), saved.host.as_str()),
            ("ada@db.example", "db.example")
        );
        assert!(saved.read_only && saved.use_tls);
        assert_eq!(saved.auth_mode, AuthMode::Password);
    }

    #[test]
    fn a_file_database_is_named_after_its_file() {
        let mut file = draft();
        file.driver_id = "sqlite".into();
        file.database = "/tmp/a.sqlite".into();
        assert_eq!(saved_from_draft(&file, Uuid::nil()).name, "/tmp/a.sqlite");
    }

    #[test]
    fn validation_names_what_is_missing() {
        let mut missing = draft();
        missing.host = " ".into();
        assert_eq!(validate(&missing).unwrap_err(), "Enter the host.");
        missing.host = "h".into();
        missing.port = 0;
        assert!(validate(&missing).unwrap_err().contains("port"));
        let mut file = draft();
        file.driver_id = "sqlite".into();
        file.database.clear();
        assert_eq!(validate(&file).unwrap_err(), "Enter the database file.");
        assert!(validate(&draft()).is_ok());
    }

    #[test]
    fn an_edit_keeps_the_fields_the_form_does_not_show() {
        let mut edit = draft();
        edit.id = Some(Uuid::nil());
        edit.keep_stored_password = true;
        let saved = saved_from_draft(&edit, Uuid::nil());
        assert_eq!(saved.id, Uuid::nil());
        assert!(edit.keep_stored_password && saved.ssh.is_none());
    }

    #[test]
    fn tls_paths_socket_and_ssh_reach_the_saved_record() {
        let mut full = draft();
        full.tls_root_cert = "/etc/ca.pem".into();
        full.tls_client_cert = "/etc/c.pem".into();
        full.tls_client_key = "/etc/c.key".into();
        full.socket_kind = Some(LocalSocketPathKind::Directory);
        full.socket_endpoint = "/run/postgresql".into();
        full.auth_mode = AuthMode::Kerberos;
        full.ssh = Some(SshDraft {
            host: "jump.example".into(),
            port: 22,
            username: "ada".into(),
            key_path: "/home/ada/.ssh/id".into(),
            secret: SecretString::from("S3cr3t-xyz".to_owned()),
        });
        let saved = saved_from_draft(&full, Uuid::nil());
        assert_eq!(
            saved.tls_root_cert.as_deref(),
            Some(std::path::Path::new("/etc/ca.pem"))
        );
        assert_eq!(
            saved.socket_dir.as_deref(),
            Some(std::path::Path::new("/run/postgresql"))
        );
        assert_eq!(saved.auth_mode, AuthMode::Kerberos);
        let ssh = saved.ssh.unwrap();
        assert!(matches!(
            ssh.auth,
            SavedSshAuth::PrivateKey {
                has_passphrase: true,
                ..
            }
        ));
        assert!(!format!("{ssh:?}").contains("S3cr3t-xyz"));
    }

    #[test]
    fn validation_rejects_half_a_client_certificate_a_relative_socket_and_a_blank_ssh_host() {
        let mut half = draft();
        half.tls_client_cert = "/etc/c.pem".into();
        assert!(validate(&half).unwrap_err().contains("client certificate"));
        let mut relative = draft();
        relative.socket_endpoint = "run/pg".into();
        assert!(validate(&relative).unwrap_err().contains("absolute"));
        let mut blank = draft();
        blank.ssh = Some(SshDraft {
            host: " ".into(),
            port: 22,
            username: "u".into(),
            key_path: String::new(),
            secret: SecretString::from(String::new()),
        });
        assert!(validate(&blank).unwrap_err().contains("SSH host"));
    }

    #[test]
    fn a_multi_hop_or_agent_chain_is_kept_not_flattened_on_edit() {
        let mut saved = saved_from_draft(&draft(), Uuid::nil());
        let mut chain = ssh_config(&SshDraft {
            host: "a".into(),
            port: 22,
            username: "u".into(),
            key_path: String::new(),
            secret: SecretString::from(String::new()),
        });
        chain.jump = Some(Box::new(chain.clone()));
        saved.ssh = Some(chain);
        let edited = draft_from_saved(&saved);
        assert!(edited.ssh.is_none() && edited.keep_saved_ssh);
    }

    #[tokio::test]
    async fn a_file_database_tests_as_connected_without_being_saved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("t.db");
        std::fs::File::create(&path).unwrap();
        let mut registry = bookie_core::DriverRegistry::new();
        registry.register(std::sync::Arc::new(drivers_sqlite::SqliteDriver));
        let mut file = draft();
        file.driver_id = "sqlite".into();
        file.database = path.display().to_string();
        let outcome = test(&registry, &file, tokio_util::sync::CancellationToken::new()).await;
        assert!(outcome.unwrap().starts_with("Connected"));
        let mut bad = draft();
        bad.driver_id = "nonesuch".into();
        assert!(
            test(&registry, &bad, tokio_util::sync::CancellationToken::new())
                .await
                .is_err()
        );
        let mut ssh = draft();
        ssh.ssh = Some(SshDraft {
            host: "h".into(),
            port: 22,
            username: "u".into(),
            key_path: String::new(),
            secret: SecretString::from(String::new()),
        });
        assert!(
            test(&registry, &ssh, tokio_util::sync::CancellationToken::new())
                .await
                .unwrap_err()
                .contains("Save the connection first")
        );
    }

    #[test]
    fn the_secret_is_never_part_of_the_saved_record() {
        let saved = saved_from_draft(&draft(), Uuid::nil());
        assert!(!format!("{saved:?}").contains("secret"));
    }
}
