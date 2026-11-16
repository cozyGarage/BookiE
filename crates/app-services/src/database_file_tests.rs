use super::*;
use drivers_sqlite::SqliteDriver;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

#[test]
fn creating_a_database_file_leaves_an_empty_owner_only_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("new.db");
    create_database_file(&SqliteDriver, &path).unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.len(), 0);
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
}

#[test]
fn creating_over_an_existing_file_is_refused_and_keeps_its_content() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("existing.db");
    std::fs::write(&path, b"existing rows").unwrap();
    let error = create_database_file(&SqliteDriver, &path).unwrap_err();
    assert!(matches!(error, CreateDatabaseFileError::AlreadyExists), "{error:?}");
    assert_eq!(std::fs::read(&path).unwrap(), b"existing rows");
}

#[test]
fn creating_through_a_dangling_link_is_refused_and_creates_nothing() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("target.db");
    let link = dir.path().join("link.db");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let error = create_database_file(&SqliteDriver, &link).unwrap_err();
    assert!(matches!(error, CreateDatabaseFileError::AlreadyExists), "{error:?}");
    assert!(!target.exists());
}

#[test]
fn creating_in_a_missing_directory_reports_the_input_output_failure() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing").join("new.db");
    let error = create_database_file(&SqliteDriver, &path).unwrap_err();
    assert!(
        matches!(&error, CreateDatabaseFileError::Io(source) if source.kind() == std::io::ErrorKind::NotFound),
        "{error:?}"
    );
    assert!(!path.exists());
}

#[test]
fn a_created_file_opens_as_a_database() {
    use bookie_core::ConnectOptions;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("new.db");
    create_database_file(&SqliteDriver, &path).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let options = ConnectOptions {
        database: path.to_string_lossy().to_string(),
        ..Default::default()
    };
    let connection = runtime.block_on(SqliteDriver.connect(options)).unwrap();
    runtime
        .block_on(connection.execute("CREATE TABLE t (id INTEGER PRIMARY KEY)"))
        .unwrap();
}

struct RefusesInitialisation;

#[async_trait::async_trait]
impl DatabaseDriver for RefusesInitialisation {
    fn id(&self) -> &'static str {
        "refuses-initialisation"
    }

    fn display_name(&self) -> &'static str {
        "Refuses initialisation"
    }

    fn default_port(&self) -> u16 {
        0
    }

    fn initialize_new_database_file(&self, _path: &Path) -> Result<(), DriverError> {
        Err(DriverError::Unsupported("initialisation".into()))
    }

    async fn connect(
        &self,
        _options: bookie_core::ConnectOptions,
    ) -> Result<Box<dyn bookie_core::Connection>, DriverError> {
        Err(DriverError::Unsupported("connect".into()))
    }
}

#[test]
fn a_file_the_engine_cannot_initialise_is_removed_again() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("new.duckdb");
    let error = create_database_file(&RefusesInitialisation, &path).unwrap_err();
    assert!(matches!(error, CreateDatabaseFileError::Initialize(_)), "{error:?}");
    assert!(!path.exists());
}
