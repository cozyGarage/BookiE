use bookie_core::{AuthMode, Environment, TlsMode};
use bookie_storage::{SavedConnection, delete_connection, delete_password, save_connections, store_password};
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

pub struct ConnectionDraft {
    pub id: Option<Uuid>,
    pub name: String,
    pub driver_id: String,
    pub file_based: bool,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: SecretString,
    pub tls_mode: TlsMode,
    pub read_only: bool,
    pub environment: Environment,
}

pub fn derived_name(file_based: bool, host: &str, username: &str, database: &str) -> String {
    if file_based {
        return database.to_owned();
    }
    format!("{username}@{host}")
}

pub fn validate(draft: &ConnectionDraft) -> Result<(), String> {
    if draft.driver_id.trim().is_empty() {
        return Err("Choose a database type.".into());
    }
    if draft.file_based {
        return if draft.database.trim().is_empty() {
            Err("Enter the database file.".into())
        } else {
            Ok(())
        };
    }
    if draft.host.trim().is_empty() {
        return Err("Enter the host.".into());
    }
    if draft.port == 0 {
        return Err("Enter a port from 1 to 65535.".into());
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
    SavedConnection {
        id,
        name,
        driver_id: draft.driver_id.clone(),
        host: draft.host.trim().to_owned(),
        port: draft.port,
        socket_dir: None,
        socket_path: None,
        database: draft.database.trim().to_owned(),
        username: draft.username.trim().to_owned(),
        use_tls: draft.tls_mode.encrypts(),
        tls_mode: Some(draft.tls_mode),
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        tls_strict_encryption: false,
        auth_mode: AuthMode::Password,
        read_only: draft.read_only,
        environment: draft.environment,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        auth_source: None,
        attached_databases: Vec::new(),
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
    let saved = saved_from_draft(&draft, draft.id.unwrap_or_else(Uuid::new_v4));
    persist_with_password(&saved, existing.as_ref(), &draft.password, &saved.name).await?;
    Ok(saved)
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
            file_based: false,
            host: " db.example ".into(),
            port: 5432,
            database: "app".into(),
            username: "ada".into(),
            password: SecretString::from("secret".to_owned()),
            tls_mode: TlsMode::VerifyFull,
            read_only: true,
            environment: Environment::Local,
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
    fn a_file_based_database_is_named_after_its_file() {
        let mut file = draft();
        file.driver_id = "future-file-database".into();
        file.file_based = true;
        file.database = "/tmp/a.sqlite".into();
        file.host.clear();
        file.port = 0;
        assert_eq!(saved_from_draft(&file, Uuid::nil()).name, "/tmp/a.sqlite");
        assert!(validate(&file).is_ok());
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
    fn the_secret_is_never_part_of_the_saved_record() {
        let saved = saved_from_draft(&draft(), Uuid::nil());
        assert!(!format!("{saved:?}").contains("secret"));
    }
}
