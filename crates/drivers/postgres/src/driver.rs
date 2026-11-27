use super::*;
use async_trait::async_trait;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, DriverValueInputCapabilities};
use secrecy::ExposeSecret;
use sqlx::postgres::PgConnectOptions;

pub struct PgDriver;

#[async_trait]
impl DatabaseDriver for PgDriver {
    fn value_input_capabilities(&self) -> DriverValueInputCapabilities {
        DriverValueInputCapabilities {
            special_numeric_literals: true,
            extended_calendar_temporals: true,
            ..DriverValueInputCapabilities::default()
        }
    }

    fn id(&self) -> &'static str {
        "postgres"
    }

    fn supports_value_search(&self) -> bool {
        true
    }

    fn import_upsert(&self) -> bookie_core::ImportUpsertStyle {
        bookie_core::ImportUpsertStyle::OnConflict
    }

    fn supports_snapshot_export(&self) -> bool {
        true
    }
    fn supports_reverse_keyset(&self) -> bool {
        true
    }

    fn supports_updatable_views(&self) -> bool {
        true
    }

    fn display_name(&self) -> &'static str {
        "PostgreSQL"
    }

    fn default_port(&self) -> u16 {
        5432
    }

    fn default_database(&self) -> &'static str {
        "postgres"
    }

    fn default_username(&self) -> &'static str {
        "postgres"
    }

    fn ddl_is_transactional(&self) -> bool {
        true
    }

    fn supports_database_listing(&self) -> bool {
        true
    }

    fn supports_index_metadata(&self) -> bool {
        true
    }

    fn supports_table_health_report(&self) -> bool {
        true
    }

    fn supports_foreign_key_metadata(&self) -> bool {
        true
    }

    fn catalog_object_kinds(&self) -> &'static [bookie_core::CatalogObjectKind] {
        catalog::SUPPORTED_KINDS
    }

    fn supports_view_metadata(&self) -> bool {
        true
    }

    fn forwarded_socket_name(&self, service_port: u16) -> Option<String> {
        Some(format!(".s.PGSQL.{service_port}"))
    }

    fn supports_local_socket(&self) -> bool {
        true
    }

    fn supports_local_socket_directory(&self) -> bool {
        true
    }

    fn supports_sequence_restart(&self) -> bool {
        true
    }

    fn supports_client_tls_auth(&self) -> bool {
        true
    }

    fn supports_explicit_auto_increment_values(&self) -> bool {
        true
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        let pg_opts = full_connect_options(&opts)?;
        let session_options = pg_opts.clone();
        let pool = pool::options(opts.pool_acquire_timeout())
            .connect_with(pg_opts)
            .await
            .map_err(map_sqlx_connect_error)?;
        Ok(Box::new(PgConnection { pool, session_options }))
    }
}

pub(crate) fn full_connect_options(opts: &ConnectOptions) -> Result<PgConnectOptions, DriverError> {
    use bookie_core::TlsMode;
    let mut pg_opts = postgres_connect_options(opts)?;
    pg_opts = pg_opts
        .database(&opts.database)
        .username(&opts.username)
        .password(opts.password.expose_secret())
        .ssl_mode(match opts.tls.mode {
            TlsMode::Disabled => sqlx::postgres::PgSslMode::Disable,
            TlsMode::Prefer => sqlx::postgres::PgSslMode::Prefer,
            TlsMode::Require => sqlx::postgres::PgSslMode::Require,
            TlsMode::VerifyCa => sqlx::postgres::PgSslMode::VerifyCa,
            TlsMode::VerifyFull => sqlx::postgres::PgSslMode::VerifyFull,
        });
    if let Some(path) = &opts.tls.root_cert {
        pg_opts = pg_opts.ssl_root_cert(path);
    }
    if let Some(path) = &opts.tls.client_cert {
        pg_opts = pg_opts.ssl_client_cert(path);
    }
    if let Some(path) = &opts.tls.client_key {
        pg_opts = pg_opts.ssl_client_key(path);
    }
    if let Some(name) = &opts.application_name {
        pg_opts = pg_opts.application_name(name);
    }
    if opts.read_only {
        pg_opts = pg_opts.options([("default_transaction_read_only", "on")]);
    }
    pg_opts = pg_opts.statement_cache_capacity(0);

    Ok(pg_opts)
}

pub(crate) fn postgres_connect_options(opts: &ConnectOptions) -> Result<PgConnectOptions, DriverError> {
    match opts.transport() {
        Transport::Tcp { host, port } => Ok(PgConnectOptions::new().host(host).port(port)),
        Transport::LocalSocketFile { .. } => {
            Err(DriverError::Unsupported("local socket files are not supported".into()))
        }
        Transport::Socket {
            directory,
            identity_host,
            identity_port,
            ..
        } => Ok(PgConnectOptions::new()
            .host(identity_host)
            .port(identity_port)
            .socket(directory)),
    }
}
