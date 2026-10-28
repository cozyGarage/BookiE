use super::*;

struct BareDriver;

#[async_trait]
impl DatabaseDriver for BareDriver {
    fn id(&self) -> &'static str {
        "bare"
    }

    fn display_name(&self) -> &'static str {
        "Bare"
    }

    fn default_port(&self) -> u16 {
        0
    }

    async fn connect(&self, _opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        Err(DriverError::Unsupported("bare".into()))
    }
}

#[test]
fn a_driver_that_declares_nothing_claims_no_structure_metadata() {
    let driver = BareDriver;
    assert!(!driver.supports_database_listing());
    assert!(!driver.supports_index_metadata());
    assert!(!driver.supports_foreign_key_metadata());
    assert!(!driver.supports_view_metadata());
    assert!(!driver.supports_column_comments());
    let capabilities = driver.capabilities();
    assert!(!capabilities.supports_database_listing);
    assert!(!capabilities.supports_index_metadata);
    assert!(!capabilities.supports_foreign_key_metadata);
    assert!(!capabilities.supports_view_metadata);
    assert_eq!(capabilities.column_comment, ColumnCommentCapability::Unsupported);
    assert!(!capabilities.supports_explicit_auto_increment_values);
    assert_eq!(driver.default_database(), "");
    assert_eq!(driver.default_username(), "");
}

struct DeclaredCapabilitiesDriver;

#[async_trait]
impl DatabaseDriver for DeclaredCapabilitiesDriver {
    fn id(&self) -> &'static str {
        "declared-capabilities"
    }

    fn display_name(&self) -> &'static str {
        "Declared capabilities"
    }

    fn default_port(&self) -> u16 {
        0
    }

    fn maturity(&self) -> DriverMaturity {
        DriverMaturity::Experimental
    }

    fn enum_input_style(&self) -> EnumInputStyle {
        EnumInputStyle::SqlQuotedLabel
    }

    fn is_file_based(&self) -> bool {
        true
    }

    fn ddl_is_transactional(&self) -> bool {
        true
    }

    fn reports_rows_affected(&self) -> bool {
        false
    }

    fn supports_database_listing(&self) -> bool {
        true
    }

    fn supports_index_metadata(&self) -> bool {
        true
    }

    fn supports_foreign_key_metadata(&self) -> bool {
        true
    }

    fn supports_view_metadata(&self) -> bool {
        true
    }

    fn catalog_object_kinds(&self) -> &'static [crate::CatalogObjectKind] {
        &[crate::CatalogObjectKind::Routine]
    }

    fn supports_integrated_auth(&self) -> bool {
        true
    }

    fn supports_local_socket(&self) -> bool {
        true
    }

    fn supports_local_socket_file(&self) -> bool {
        true
    }

    fn supports_local_socket_directory(&self) -> bool {
        true
    }

    fn supports_client_tls_auth(&self) -> bool {
        true
    }

    fn supports_explicit_auto_increment_values(&self) -> bool {
        true
    }

    async fn connect(&self, _opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        Err(DriverError::Unsupported("test driver".into()))
    }
}

#[test]
fn the_static_capability_profile_reflects_driver_declarations() {
    let capabilities = DeclaredCapabilitiesDriver.capabilities();

    assert_eq!(capabilities.maturity, DriverMaturity::Experimental);
    assert_eq!(capabilities.enum_input_style, EnumInputStyle::SqlQuotedLabel);
    assert!(capabilities.is_file_based);
    assert!(capabilities.ddl_is_transactional);
    assert!(!capabilities.reports_rows_affected);
    assert!(capabilities.supports_database_listing);
    assert!(capabilities.supports_index_metadata);
    assert!(capabilities.supports_foreign_key_metadata);
    assert!(capabilities.supports_view_metadata);
    assert_eq!(capabilities.catalog_object_kinds, &[crate::CatalogObjectKind::Routine]);
    assert!(capabilities.supports_integrated_auth);
    assert!(capabilities.supports_local_socket);
    assert!(capabilities.supports_local_socket_file);
    assert!(capabilities.supports_local_socket_directory);
    assert!(capabilities.supports_client_tls_auth);
    assert!(capabilities.supports_explicit_auto_increment_values);
}

struct NamedDriver {
    name: &'static str,
    port: u16,
    database: &'static str,
    username: &'static str,
}

#[async_trait]
impl DatabaseDriver for NamedDriver {
    fn id(&self) -> &'static str {
        self.name
    }

    fn display_name(&self) -> &'static str {
        self.name
    }

    fn default_port(&self) -> u16 {
        self.port
    }

    fn default_database(&self) -> &'static str {
        self.database
    }

    fn default_username(&self) -> &'static str {
        self.username
    }

    async fn connect(&self, _opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        Err(DriverError::Unsupported(self.name.into()))
    }
}

#[test]
fn the_first_sorted_driver_supplies_the_connection_form() {
    let clickhouse = NamedDriver {
        name: "ClickHouse",
        port: 8123,
        database: "default",
        username: "default",
    };
    let postgres = NamedDriver {
        name: "PostgreSQL",
        port: 5432,
        database: "postgres",
        username: "postgres",
    };
    let drivers: [&dyn DatabaseDriver; 2] = [&postgres, &clickhouse];
    let mut sorted = drivers;
    sorted.sort_by(|left, right| left.display_name().cmp(right.display_name()));

    let initial = initial_connection_form_defaults(&drivers).unwrap();
    assert_eq!(initial, connection_form_defaults(sorted[0]));
    assert_eq!(initial.port, 8123);
    assert_eq!(initial.database, "default");
    assert_eq!(initial.username, "default");
    assert_ne!(
        (initial.port, initial.database, initial.username),
        (5432, "postgres", "postgres")
    );

    let switched = connection_form_defaults(&postgres);
    assert_eq!(switched.port, 5432);
    assert_eq!(switched.database, "postgres");
    assert_eq!(switched.username, "postgres");
    assert_ne!(switched, initial);
}
