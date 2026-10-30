use async_trait::async_trait;

use crate::connection::{ConnectOptions, Connection};
use crate::error::DriverError;
use crate::sql_ddl::ColumnCommentCapability;

/// How complete a driver is for day-to-day use.
///
/// `Stable` drivers support browse, SQL (or the engine's query dialect),
/// and the common write paths. `Experimental` drivers connect and browse
/// but may lack parameter binding, interactive transactions, or full
/// write coverage. See `docs/adding-drivers.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DriverMaturity {
    #[default]
    Stable,
    Experimental,
}

impl DriverMaturity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stable => "Stable",
            Self::Experimental => "Experimental",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnumInputStyle {
    #[default]
    RawValue,
    DeclaredMemberList,
    SqlQuotedLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DriverCapabilities {
    pub maturity: DriverMaturity,
    pub enum_input_style: EnumInputStyle,
    pub is_file_based: bool,
    pub ddl_is_transactional: bool,
    pub reports_rows_affected: bool,
    pub supports_database_listing: bool,
    pub supports_index_metadata: bool,
    pub supports_foreign_key_metadata: bool,
    pub column_comment: ColumnCommentCapability,
    pub supports_view_metadata: bool,
    pub catalog_object_kinds: &'static [crate::CatalogObjectKind],
    pub supports_integrated_auth: bool,
    pub supports_local_socket: bool,
    pub supports_local_socket_file: bool,
    pub supports_local_socket_directory: bool,
    pub supports_client_tls_auth: bool,
    pub supports_explicit_auto_increment_values: bool,
}

#[async_trait]
pub trait DatabaseDriver: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn default_port(&self) -> u16;

    fn default_database(&self) -> &'static str {
        ""
    }

    fn default_username(&self) -> &'static str {
        ""
    }

    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities {
            maturity: self.maturity(),
            enum_input_style: self.enum_input_style(),
            is_file_based: self.is_file_based(),
            ddl_is_transactional: self.ddl_is_transactional(),
            reports_rows_affected: self.reports_rows_affected(),
            supports_database_listing: self.supports_database_listing(),
            supports_index_metadata: self.supports_index_metadata(),
            supports_foreign_key_metadata: self.supports_foreign_key_metadata(),
            column_comment: crate::sql_ddl::column_comment_capability(self.id()),
            supports_view_metadata: self.supports_view_metadata(),
            catalog_object_kinds: self.catalog_object_kinds(),
            supports_integrated_auth: self.supports_integrated_auth(),
            supports_local_socket: self.supports_local_socket(),
            supports_local_socket_file: self.supports_local_socket_file(),
            supports_local_socket_directory: self.supports_local_socket_directory(),
            supports_client_tls_auth: self.supports_client_tls_auth(),
            supports_explicit_auto_increment_values: self.supports_explicit_auto_increment_values(),
        }
    }

    fn maturity(&self) -> DriverMaturity {
        DriverMaturity::Stable
    }

    fn enum_input_style(&self) -> EnumInputStyle {
        EnumInputStyle::RawValue
    }

    fn is_file_based(&self) -> bool {
        false
    }

    /// Whether a multi-statement DDL batch can roll back as a unit, so
    /// the structure editor's Save runs through
    /// `Connection::execute_in_transaction` instead of statement by
    /// statement. MySQL commits implicitly on every DDL statement: the
    /// transaction would end after the first one and a later failure
    /// would leave the earlier statements applied, which is worse than
    /// not opening one at all.
    fn ddl_is_transactional(&self) -> bool {
        false
    }

    /// Whether `ExecResult::rows_affected` carries a real count for
    /// UPDATE and DELETE. The inline-edit Save path reads a zero count
    /// as an optimistic-concurrency conflict, so a driver that cannot
    /// produce one must say so or every successful save reports a lost
    /// update. ClickHouse applies both as asynchronous mutations and
    /// returns no row count for either.
    fn reports_rows_affected(&self) -> bool {
        true
    }

    /// Whether `Connection::list_databases` answers with the databases the
    /// connected account can open. Engines with a single database per
    /// connection, such as the file-based ones, declare false.
    fn supports_database_listing(&self) -> bool {
        false
    }

    /// Whether this driver can enumerate the indexes on a table.
    /// `Connection::fetch_indexes` answers with an empty list when it
    /// cannot, and the structure tab has no way to tell that apart from
    /// a table that genuinely has no index. A driver must declare true
    /// only when it implements the fetch against real catalog data.
    fn supports_index_metadata(&self) -> bool {
        false
    }

    /// Whether this driver can enumerate the foreign keys on a table,
    /// for the same reason as `supports_index_metadata`. Engines with no
    /// foreign-key concept declare false even when the driver overrides
    /// the fetch to return an empty list.
    fn supports_foreign_key_metadata(&self) -> bool {
        false
    }

    /// Whether this driver's engine stores a comment on a column. The
    /// answer comes from the DDL builder's placement table rather than
    /// a per-driver override, so a driver cannot advertise a capability
    /// the builder would refuse to emit.
    fn supports_column_comments(&self) -> bool {
        crate::sql_ddl::column_comment_capability(self.id()).is_supported()
    }

    fn supports_view_metadata(&self) -> bool {
        false
    }

    fn catalog_object_kinds(&self) -> &'static [crate::CatalogObjectKind] {
        &[]
    }

    fn supports_integrated_auth(&self) -> bool {
        false
    }

    fn supports_local_socket(&self) -> bool {
        false
    }

    fn supports_local_socket_file(&self) -> bool {
        false
    }

    fn supports_local_socket_directory(&self) -> bool {
        false
    }

    /// Whether this driver can authenticate TLS connections with a client
    /// certificate and private key. Transport refuses configured identities
    /// for drivers that do not explicitly opt in.
    fn supports_client_tls_auth(&self) -> bool {
        false
    }

    fn supports_explicit_auto_increment_values(&self) -> bool {
        false
    }

    /// File name a forwarded Unix socket must use for this driver to
    /// dial it while verifying TLS against the original service
    /// hostname. `None` means the driver has no socket transport and an
    /// SSH tunnel must forward TCP instead.
    fn forwarded_socket_name(&self, _service_port: u16) -> Option<String> {
        None
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionFormDefaults {
    pub port: u16,
    pub database: &'static str,
    pub username: &'static str,
}

pub fn connection_form_defaults(driver: &dyn DatabaseDriver) -> ConnectionFormDefaults {
    ConnectionFormDefaults {
        port: driver.default_port(),
        database: driver.default_database(),
        username: driver.default_username(),
    }
}

pub fn initial_connection_form_defaults(drivers: &[&dyn DatabaseDriver]) -> Option<ConnectionFormDefaults> {
    drivers
        .iter()
        .min_by(|left, right| left.display_name().cmp(right.display_name()))
        .map(|driver| connection_form_defaults(*driver))
}

#[cfg(test)]
#[path = "driver_tests.rs"]
mod tests;
