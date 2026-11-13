use std::path::Path;

use serde::Deserialize;
use serde::de::IgnoredAny;
use sqlx::Connection;
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};

use crate::connection_import_peer::{PeerImport, PeerPort, field, port, refused, skipped_label};
use crate::connection_url::{draft_connection, validate_host};
use crate::error::StorageError;
use bookie_core::TlsMode;

pub const MAX_DBX_CONFIG_BYTES: usize = 1_048_576;
const MAX_DBX_TOTAL_CONFIG_BYTES: usize = 4 * 1024 * 1024;
const MAX_DBX_CONNECTIONS: usize = 1_000;

#[derive(Deserialize)]
struct PeerConnection {
    #[serde(default)]
    name: String,
    #[serde(default)]
    db_type: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    port: Option<PeerPort>,
    #[serde(default)]
    username: String,
    #[serde(default)]
    database: Option<String>,
    #[serde(default)]
    driver_profile: Option<String>,
    #[serde(default)]
    url_params: Option<String>,
    #[serde(default)]
    transport_layers: Vec<IgnoredAny>,
    #[serde(default)]
    connection_string: Option<String>,
    #[serde(default)]
    init_script: Option<String>,
    #[serde(default)]
    attached_databases: Vec<IgnoredAny>,
    #[serde(default)]
    default_schema: Option<String>,
    #[serde(default)]
    visible_databases: Option<Vec<IgnoredAny>>,
    #[serde(default)]
    visible_database_patterns: Option<Vec<IgnoredAny>>,
    #[serde(default)]
    visible_schemas: Option<IgnoredAny>,
    #[serde(default)]
    production_databases: Vec<IgnoredAny>,
    #[serde(default)]
    production_badge_scope: Option<IgnoredAny>,
    #[serde(default)]
    is_production: bool,
    #[serde(default)]
    agent_java_options: Vec<IgnoredAny>,
    #[serde(default)]
    jdbc_driver_class: Option<String>,
    #[serde(default)]
    jdbc_driver_paths: Vec<IgnoredAny>,
    #[serde(default)]
    external_config: Option<IgnoredAny>,
    #[serde(default)]
    plugin_id: Option<String>,
    #[serde(default)]
    plugin_connection_provider: Option<String>,
    #[serde(default)]
    plugin_connection_type: Option<String>,
    #[serde(default)]
    ssl: bool,
    #[serde(default)]
    ca_cert_path: String,
    #[serde(default)]
    client_cert_path: String,
    #[serde(default)]
    client_key_path: String,
    #[serde(default = "default_connect_timeout_secs")]
    connect_timeout_secs: u64,
    #[serde(default = "default_query_timeout_secs")]
    query_timeout_secs: u64,
    #[serde(default)]
    read_only: bool,
    #[serde(default)]
    redis_connection_mode: Option<String>,
    #[serde(default)]
    redis_sentinel_master: String,
    #[serde(default)]
    redis_sentinel_nodes: String,
    #[serde(default)]
    redis_sentinel_username: String,
    #[serde(default)]
    redis_sentinel_tls: bool,
    #[serde(default)]
    redis_cluster_nodes: String,
}

fn default_connect_timeout_secs() -> u64 {
    10
}

fn default_query_timeout_secs() -> u64 {
    60
}

pub async fn parse_dbx_connection_database(path: &Path) -> Result<PeerImport, StorageError> {
    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    let mut db = SqliteConnection::connect_with(&options).await?;
    sqlx::query("BEGIN").execute(&mut db).await?;
    verify_schema(&mut db).await?;
    let result = read_connections(&mut db).await;
    sqlx::query("ROLLBACK").execute(&mut db).await?;
    result
}

async fn verify_schema(db: &mut SqliteConnection) -> Result<(), StorageError> {
    let columns = sqlx::query("PRAGMA table_info(connections)")
        .fetch_all(db)
        .await?
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .collect::<Vec<_>>();
    if columns.iter().any(|column| column == "id") && columns.iter().any(|column| column == "config_json") {
        return Ok(());
    }
    Err(refused("the selected SQLite file does not contain DBX connection data"))
}

async fn read_connections(db: &mut SqliteConnection) -> Result<PeerImport, StorageError> {
    let mut drafts = Vec::new();
    let mut skipped = Vec::new();
    let mut total_bytes = 0_usize;
    for offset in 0..=MAX_DBX_CONNECTIONS {
        let row = sqlx::query(
            "SELECT id, CASE WHEN length(CAST(config_json AS BLOB)) <= ? \
             THEN CAST(config_json AS BLOB) ELSE X'' END AS config, \
             length(CAST(config_json AS BLOB)) AS config_bytes FROM connections LIMIT 1 OFFSET ?",
        )
        .bind(i64::try_from(MAX_DBX_CONFIG_BYTES).unwrap_or(i64::MAX))
        .bind(i64::try_from(offset).unwrap_or(i64::MAX))
        .fetch_optional(&mut *db)
        .await?;
        let Some(row) = row else { break };
        if offset == MAX_DBX_CONNECTIONS {
            return Err(refused("the DBX database lists too many connections to import"));
        }
        import_connection_row(row, &mut total_bytes, &mut drafts, &mut skipped)?;
    }
    if drafts.is_empty() {
        return Err(refused(
            "no DBX connection uses a directly supported BookiE database driver",
        ));
    }
    drafts.sort_by(|left, right| left.name.cmp(&right.name));
    skipped.sort();
    Ok(PeerImport {
        drafts,
        skipped,
        groups: Default::default(),
    })
}

fn import_connection_row(
    row: sqlx::sqlite::SqliteRow,
    total_bytes: &mut usize,
    drafts: &mut Vec<crate::connections::SavedConnection>,
    skipped: &mut Vec<String>,
) -> Result<(), StorageError> {
    let id = row.try_get::<String, _>("id")?;
    let bytes = row.try_get::<Vec<u8>, _>("config")?;
    let size = row.try_get::<i64, _>("config_bytes")?;
    let config_size = usize::try_from(size).ok().filter(|size| *size <= MAX_DBX_CONFIG_BYTES);
    let Some(config_size) = config_size else {
        skipped.push(format!(
            "{} (configuration exceeds the import size limit)",
            skipped_label(&id, "Unnamed connection")
        ));
        return Ok(());
    };
    *total_bytes = total_bytes.saturating_add(config_size);
    if *total_bytes > MAX_DBX_TOTAL_CONFIG_BYTES {
        return Err(refused("the DBX connection data exceeds the total import size limit"));
    }
    let peer = match serde_json::from_slice::<PeerConnection>(&bytes) {
        Ok(peer) => peer,
        Err(_) => {
            skipped.push(format!(
                "{} (connection settings are not supported)",
                skipped_label(&id, "Unnamed connection")
            ));
            return Ok(());
        }
    };
    let label = display_name(&peer.name, &id);
    if driver_id(&peer.db_type).is_none() {
        let engine = field(&peer.db_type).unwrap_or_else(|| "unknown".to_string());
        skipped.push(format!("{label} (unsupported database engine: {engine})"));
        return Ok(());
    }
    match draft_from(peer) {
        Ok(draft) => drafts.push(draft),
        Err(reason) => skipped.push(format!("{label} ({reason})")),
    }
    Ok(())
}

fn draft_from(peer: PeerConnection) -> Result<crate::connections::SavedConnection, &'static str> {
    let driver_id = driver_id(&peer.db_type).ok_or("unsupported database engine")?;
    let reason = unsupported_setting(&peer);
    if let Some(reason) = reason {
        return Err(reason);
    }
    let host = validate_host(&peer.host).map_err(|_| "invalid host")?;
    let port = port(peer.port.as_ref(), driver_id).ok_or("invalid port")?;
    let name = import_field(&peer.name, &host).ok_or("invalid connection name")?;
    let username = field(&peer.username).ok_or("invalid username")?;
    let database = peer.database.as_deref().and_then(field).unwrap_or_default();
    let mut draft = draft_connection(driver_id, name, host, port, database, username);
    draft.read_only = peer.read_only;
    if peer.ssl {
        draft.use_tls = true;
        draft.tls_mode = Some(TlsMode::VerifyFull);
    }
    if peer.is_production {
        draft.environment = bookie_core::Environment::Prod;
    }
    if !peer.ca_cert_path.is_empty() {
        draft.tls_root_cert = Some(std::path::PathBuf::from(
            field(&peer.ca_cert_path).ok_or("invalid CA path")?,
        ));
    }
    draft.connect_timeout_secs = imported_timeout(peer.connect_timeout_secs)?;
    draft.query_timeout_secs = imported_timeout(peer.query_timeout_secs)?;
    Ok(draft)
}

fn driver_id(db_type: &str) -> Option<&'static str> {
    match db_type {
        "postgres" | "postgresql" => Some("postgres"),
        "mysql" | "mariadb" => Some("mysql"),
        "sqlserver" | "mssql" => Some("mssql"),
        "clickhouse" => Some("clickhouse"),
        "mongodb" => Some("mongodb"),
        "redis" => Some("redis"),
        _ => None,
    }
}

fn unsupported_setting(peer: &PeerConnection) -> Option<&'static str> {
    unsupported_connection_options(peer).or_else(|| unsupported_driver_options(peer))
}

fn unsupported_connection_options(peer: &PeerConnection) -> Option<&'static str> {
    if !peer.transport_layers.is_empty() {
        return Some("SSH or proxy transport is unsupported");
    }
    if peer.driver_profile.as_deref().is_some_and(|value| !value.is_empty()) {
        return Some("custom driver profiles are unsupported");
    }
    if peer.url_params.as_deref().is_some_and(|value| !value.is_empty()) {
        return Some("custom URL parameters are unsupported");
    }
    if peer.connection_string.as_deref().is_some_and(|value| !value.is_empty()) {
        return Some("custom connection strings are unsupported");
    }
    if peer
        .init_script
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Some("connection initialization scripts are unsupported");
    }
    if !peer.attached_databases.is_empty() || peer.default_schema.as_deref().is_some_and(|value| !value.is_empty()) {
        return Some("attached databases or a default schema are unsupported");
    }
    if peer.visible_databases.as_ref().is_some_and(|values| !values.is_empty())
        || peer
            .visible_database_patterns
            .as_ref()
            .is_some_and(|values| !values.is_empty())
        || peer.visible_schemas.is_some()
    {
        return Some("database visibility filters are unsupported");
    }
    if !peer.production_databases.is_empty() || peer.production_badge_scope.is_some() {
        return Some("database-specific production markings are unsupported");
    }
    if !peer.client_cert_path.is_empty() || !peer.client_key_path.is_empty() {
        return Some("client certificate authentication is unsupported");
    }
    None
}

fn unsupported_driver_options(peer: &PeerConnection) -> Option<&'static str> {
    if !peer.agent_java_options.is_empty()
        || peer.jdbc_driver_class.as_deref().is_some_and(|value| !value.is_empty())
        || !peer.jdbc_driver_paths.is_empty()
        || peer.external_config.is_some()
        || peer.plugin_id.as_deref().is_some_and(|value| !value.is_empty())
        || peer
            .plugin_connection_provider
            .as_deref()
            .is_some_and(|value| !value.is_empty())
        || peer
            .plugin_connection_type
            .as_deref()
            .is_some_and(|value| !value.is_empty())
    {
        return Some("custom driver configuration is unsupported");
    }
    if peer
        .redis_connection_mode
        .as_deref()
        .is_some_and(|value| !value.is_empty() && value != "standalone")
        || !peer.redis_sentinel_master.is_empty()
        || !peer.redis_sentinel_nodes.is_empty()
        || !peer.redis_sentinel_username.is_empty()
        || peer.redis_sentinel_tls
        || !peer.redis_cluster_nodes.is_empty()
    {
        return Some("Redis Sentinel or cluster settings are unsupported");
    }
    None
}

fn import_field(value: &str, fallback: &str) -> Option<String> {
    if value.trim().is_empty() {
        field(fallback)
    } else {
        field(value.trim())
    }
}

fn imported_timeout(value: u64) -> Result<Option<u32>, &'static str> {
    if value == 0 {
        return Ok(None);
    }
    u32::try_from(value)
        .map(Some)
        .map_err(|_| "timeout exceeds BookiE's supported range")
}

fn display_name(name: &str, id: &str) -> String {
    skipped_label(name, id)
}

#[cfg(test)]
#[path = "connection_import_dbx_tests.rs"]
mod tests;
