use bookie_core::{AuthMode, Environment, TlsMode};
use std::path::PathBuf;

use bookie_storage::{
    SavedConnection, SavedSshAuth, SavedSshConfig, delete_connection, delete_password, save_connections, store_password,
};

use crate::connection_form::LocalSocketPathKind;
use crate::ssh_chain_form::{ChainPlan, JumpAuth, JumpHopForm, plan_chain};
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshAuthKind {
    Password,
    KeyFile,
    Agent,
}

pub struct SshHopDraft {
    pub hop_id: Option<Uuid>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuthKind,
    pub key_path: String,
    pub agent_identity: String,
    pub secret: SecretString,
    pub stored_secret: bool,
}

pub struct SshDraft {
    pub head: SshHopDraft,
    pub openssh: bool,
    pub jumps: Vec<SshHopDraft>,
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
    pub file_based: bool,
    pub socket_kind: Option<LocalSocketPathKind>,
    pub socket_endpoint: String,
    pub ssh: Option<SshDraft>,
    pub read_only: bool,
    pub environment: Environment,
    pub keep_stored_password: bool,
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
        file_based: false,
        socket_kind,
        socket_endpoint,
        ssh: saved.ssh.as_ref().map(ssh_draft_from_saved),
        read_only: saved.read_only,
        environment: saved.environment,
        keep_stored_password: true,
    }
}

fn hop_draft_from_saved(hop: &SavedSshConfig) -> SshHopDraft {
    let (auth, key_path, stored_secret) = match (&hop.auth, hop.agent) {
        (SavedSshAuth::PrivateKey { path, .. }, true) => (SshAuthKind::Agent, path.display().to_string(), false),
        (SavedSshAuth::PrivateKey { path, has_passphrase }, false) => {
            (SshAuthKind::KeyFile, path.display().to_string(), *has_passphrase)
        }
        (SavedSshAuth::Password, _) => (SshAuthKind::Password, String::new(), true),
    };
    SshHopDraft {
        hop_id: Some(hop.hop_id),
        host: hop.host.clone(),
        port: hop.port,
        username: hop.username.clone(),
        auth,
        key_path,
        agent_identity: path_text(hop.agent_identity.as_ref()),
        secret: SecretString::from(String::new()),
        stored_secret,
    }
}

fn ssh_draft_from_saved(ssh: &SavedSshConfig) -> SshDraft {
    SshDraft {
        head: hop_draft_from_saved(ssh),
        openssh: ssh.client == bookie_storage::SshClient::OpenSsh,
        jumps: ssh
            .flatten_hops()
            .into_iter()
            .skip(1)
            .map(hop_draft_from_saved)
            .collect(),
    }
}

pub fn derived_name(file_based: bool, host: &str, username: &str, database: &str) -> String {
    if file_based {
        return database.to_owned();
    }
    format!("{username}@{host}")
}

pub fn validate(draft: &ConnectionDraft) -> Result<(), String> {
    let file_based = draft.file_based;
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
    for (index, hop) in std::iter::once(&ssh.head).chain(ssh.jumps.iter()).enumerate() {
        if hop.host.trim().is_empty() || hop.username.trim().is_empty() {
            return Err(if index == 0 {
                "Enter the SSH host and user.".into()
            } else {
                format!("Enter the host and user of jump host {index}.")
            });
        }
        if hop.port == 0 {
            return Err("Enter an SSH port from 1 to 65535.".into());
        }
        if hop.auth == SshAuthKind::KeyFile && hop.key_path.trim().is_empty() {
            return Err("Enter the private key file.".into());
        }
    }
    Ok(())
}

pub fn saved_from_draft(draft: &ConnectionDraft, id: Uuid) -> SavedConnection {
    let name = match draft.name.trim() {
        "" => derived_name(
            draft.file_based,
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
        ssh: draft.ssh.as_ref().map(|ssh| head_config(ssh, None)),
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        auth_source: None,
        attached_databases: Vec::new(),
    }
}

fn head_config(ssh: &SshDraft, previous: Option<&SavedSshConfig>) -> SavedSshConfig {
    let hop = &ssh.head;
    let agent = hop.auth == SshAuthKind::Agent;
    let identity = hop.agent_identity.trim();
    SavedSshConfig {
        hop_id: hop
            .hop_id
            .or(previous.map(|old| old.hop_id))
            .unwrap_or_else(Uuid::new_v4),
        credential_revision: previous.map_or(0, |old| old.credential_revision),
        host: hop.host.trim().to_owned(),
        port: hop.port,
        username: hop.username.trim().to_owned(),
        auth: match hop.auth {
            SshAuthKind::Password => SavedSshAuth::Password,
            SshAuthKind::KeyFile => SavedSshAuth::PrivateKey {
                path: PathBuf::from(hop.key_path.trim()),
                has_passphrase: !hop.secret.expose_secret().is_empty() || hop.stored_secret,
            },
            SshAuthKind::Agent => SavedSshAuth::PrivateKey {
                path: PathBuf::from(hop.key_path.trim()),
                has_passphrase: false,
            },
        },
        jump: None,
        client: if ssh.openssh {
            bookie_storage::SshClient::OpenSsh
        } else {
            bookie_storage::SshClient::Builtin
        },
        agent,
        agent_identity: (agent && !identity.is_empty()).then(|| PathBuf::from(identity)),
        local_port: None,
    }
}

fn jump_forms(ssh: &SshDraft) -> Vec<JumpHopForm> {
    ssh.jumps
        .iter()
        .map(|hop| {
            let entered = (!hop.secret.expose_secret().is_empty())
                .then(|| SecretString::from(hop.secret.expose_secret().to_owned()));
            JumpHopForm {
                hop_id: hop.hop_id,
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth: match hop.auth {
                    SshAuthKind::Password => JumpAuth::Password,
                    SshAuthKind::KeyFile => JumpAuth::PrivateKey,
                    SshAuthKind::Agent => JumpAuth::Agent,
                },
                key_path: hop.key_path.clone(),
                agent_identity: hop.agent_identity.clone(),
                password: if hop.auth == SshAuthKind::Password {
                    entered.clone()
                } else {
                    None
                },
                passphrase: if hop.auth == SshAuthKind::KeyFile {
                    entered
                } else {
                    None
                },
                stored_password: hop.auth == SshAuthKind::Password && hop.stored_secret,
                stored_passphrase: hop.auth == SshAuthKind::KeyFile && hop.stored_secret,
            }
        })
        .collect()
}

pub enum HeadSecret {
    None,
    Password(SecretString),
    Passphrase(SecretString),
}

fn head_secret(ssh: &SshDraft) -> HeadSecret {
    let secret = ssh.head.secret.expose_secret();
    if secret.is_empty() {
        return HeadSecret::None;
    }
    let copy = SecretString::from(secret.to_owned());
    match ssh.head.auth {
        SshAuthKind::Password => HeadSecret::Password(copy),
        SshAuthKind::KeyFile => HeadSecret::Passphrase(copy),
        SshAuthKind::Agent => HeadSecret::None,
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
        carry_over(&mut saved, previous);
    }
    let plan = match &draft.ssh {
        Some(ssh) => {
            let previous = existing.as_ref().and_then(|saved| saved.ssh.as_ref());
            let plan = plan_chain(head_config(ssh, previous), &jump_forms(ssh), previous)
                .map_err(|error| error.to_string())?;
            saved.ssh = Some(plan.head.clone());
            Some((plan, head_secret(ssh)))
        }
        None => None,
    };
    let keep_password = draft.keep_stored_password && draft.password.expose_secret().is_empty() && existing.is_some();
    let password = (!keep_password).then_some(&draft.password);
    let ssh = plan.as_ref().map(|(plan, secret)| (plan, secret));
    persist_connection(&saved, existing.as_ref(), password, &saved.name, ssh).await?;
    Ok(saved)
}

fn carry_over(saved: &mut SavedConnection, previous: &SavedConnection) {
    saved.auth_source = previous.auth_source.clone();
    saved.connect_timeout_secs = previous.connect_timeout_secs;
    saved.query_timeout_secs = previous.query_timeout_secs;
    saved.attached_databases = previous.attached_databases.clone();
    saved.tls_strict_encryption = previous.tls_strict_encryption;
    saved.last_opened_at = previous.last_opened_at;
}

pub async fn persist_connection(
    saved: &SavedConnection,
    existing: Option<&SavedConnection>,
    password: Option<&SecretString>,
    label: &str,
    ssh: Option<(&ChainPlan, &HeadSecret)>,
) -> Result<(), String> {
    let committed = match ssh {
        Some((plan, _)) => Some(
            crate::ssh_chain_secrets::commit_hop_secrets(saved.id, plan, label)
                .await
                .map_err(|error| format!("The SSH jump host secrets were not saved: {error}"))?,
        ),
        None => None,
    };
    let stored = match password {
        Some(password) => persist_with_password(saved, existing, password, label).await,
        None => save_connections(std::slice::from_ref(saved))
            .await
            .map_err(|error| format!("save: {error}")),
    };
    if let Err(error) = stored {
        if let Some(committed) = &committed
            && let Err(rollback) = crate::ssh_chain_secrets::rollback_hop_secrets(saved.id, committed).await
        {
            tracing::warn!(%rollback, "jump host secrets from a failed save could not be removed");
        }
        return Err(error);
    }
    if let Some((plan, secret)) = ssh {
        store_head_secret(saved.id, secret, label).await?;
        if let Err(error) = crate::ssh_chain_secrets::delete_obsolete_hop_secrets(saved.id, plan).await {
            tracing::warn!(%error, "replaced jump host secrets could not be removed");
        }
    }
    Ok(())
}

async fn store_head_secret(id: Uuid, secret: &HeadSecret, label: &str) -> Result<(), String> {
    match secret {
        HeadSecret::None => Ok(()),
        HeadSecret::Password(password) => bookie_storage::store_ssh_password(id, password.expose_secret(), label)
            .await
            .map_err(|error| format!("Connection settings were saved, but the SSH password was not saved: {error}. Re-enter the SSH password and connect again.")),
        HeadSecret::Passphrase(passphrase) => bookie_storage::store_ssh_passphrase(id, passphrase.expose_secret(), label)
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
    audit: Option<&bookie_transport::TransportAuditFactory>,
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
    let mut environment = bookie_transport::SshEnvironment::builtin(bookie_ssh::UnknownHostKey::Refuse);
    if let Some(audit) = audit {
        environment = environment.with_audit(audit.context(
            bookie_policy::Principal::human_gui(),
            saved.id,
            saved.name.clone(),
            saved.environment,
            saved.driver_id.clone(),
        ));
    }
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

    fn hop(host: &str, auth: SshAuthKind) -> SshHopDraft {
        SshHopDraft {
            hop_id: None,
            host: host.into(),
            port: 22,
            username: "ada".into(),
            auth,
            key_path: if auth == SshAuthKind::KeyFile {
                "/home/ada/.ssh/id".into()
            } else {
                String::new()
            },
            agent_identity: String::new(),
            secret: SecretString::from(String::new()),
            stored_secret: false,
        }
    }

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
            file_based: false,
            socket_kind: None,
            socket_endpoint: String::new(),
            ssh: None,
            read_only: true,
            environment: Environment::Local,
            keep_stored_password: false,
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
        file.file_based = true;
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
        file.file_based = true;
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
    fn an_empty_socket_field_saves_no_socket_even_when_the_driver_offers_one() {
        let mut draft = draft();
        draft.socket_kind = Some(LocalSocketPathKind::Directory);
        draft.socket_endpoint = "  ".into();
        let saved = saved_from_draft(&draft, Uuid::new_v4());
        assert!(saved.socket_dir.is_none() && saved.socket_path.is_none());
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
        let mut head = hop("jump.example", SshAuthKind::KeyFile);
        head.secret = SecretString::from("S3cr3t-xyz".to_owned());
        full.ssh = Some(SshDraft {
            head,
            openssh: false,
            jumps: Vec::new(),
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
            head: hop(" ", SshAuthKind::Password),
            openssh: false,
            jumps: Vec::new(),
        });
        assert!(validate(&blank).unwrap_err().contains("SSH host"));
        let mut jump = draft();
        jump.ssh = Some(SshDraft {
            head: hop("a", SshAuthKind::Password),
            openssh: false,
            jumps: vec![hop(" ", SshAuthKind::Agent)],
        });
        assert!(validate(&jump).unwrap_err().contains("jump host 1"));
    }

    #[test]
    fn a_jump_chain_an_agent_and_the_openssh_client_round_trip_through_the_saved_record() {
        let mut chain = draft();
        chain.ssh = Some(SshDraft {
            head: hop("db-gate", SshAuthKind::Agent),
            openssh: false,
            jumps: vec![hop("bastion", SshAuthKind::KeyFile), hop("edge", SshAuthKind::Password)],
        });
        chain.ssh.as_mut().unwrap().jumps[1].secret = SecretString::from("pw".to_owned());
        let ssh = chain.ssh.as_ref().unwrap();
        let plan = plan_chain(head_config(ssh, None), &jump_forms(ssh), None).unwrap();
        let mut saved = saved_from_draft(&chain, Uuid::nil());
        saved.ssh = Some(plan.head);
        let back = draft_from_saved(&saved).ssh.unwrap();
        assert_eq!(back.head.auth, SshAuthKind::Agent);
        assert_eq!(back.jumps.len(), 2);
        assert_eq!(back.jumps[0].host, "bastion");
        assert_eq!(back.jumps[0].auth, SshAuthKind::KeyFile);
        assert_eq!(back.jumps[1].auth, SshAuthKind::Password);
        let mut openssh = chain.ssh.take().unwrap();
        openssh.openssh = true;
        let refused = plan_chain(head_config(&openssh, None), &jump_forms(&openssh), None);
        assert!(refused.is_err());
    }

    #[tokio::test]
    async fn a_file_database_tests_as_connected_without_being_saved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("t.db");
        std::fs::File::create(&path).unwrap();
        let mut registry = bookie_core::DriverRegistry::new();
        registry.register(std::sync::Arc::new(drivers_sqlite::SqliteDriver));
        let cancel = || tokio_util::sync::CancellationToken::new();
        let mut file = draft();
        file.driver_id = "sqlite".into();
        file.file_based = true;
        file.database = path.display().to_string();
        assert!(
            test(&registry, &file, cancel(), None)
                .await
                .unwrap()
                .starts_with("Connected")
        );
        let mut bad = draft();
        bad.driver_id = "nonesuch".into();
        assert!(test(&registry, &bad, cancel(), None).await.is_err());
        let mut ssh = draft();
        ssh.ssh = Some(SshDraft {
            head: hop("h", SshAuthKind::Password),
            openssh: false,
            jumps: Vec::new(),
        });
        assert!(
            test(&registry, &ssh, cancel(), None)
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
