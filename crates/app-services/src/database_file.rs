use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use bookie_core::{DatabaseDriver, DriverError};

const OWNER_READ_WRITE: u32 = 0o600;

#[derive(Debug, thiserror::Error)]
pub enum CreateDatabaseFileError {
    #[error("the database file already exists")]
    AlreadyExists,
    #[error("the database file could not be created: {0}")]
    Io(#[source] std::io::Error),
    #[error("the database file could not be initialised: {0}")]
    Initialize(#[source] DriverError),
}

pub fn create_database_file(driver: &dyn DatabaseDriver, path: &Path) -> Result<(), CreateDatabaseFileError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_READ_WRITE)
        .open(path)
        .map_err(|error| match error.kind() {
            ErrorKind::AlreadyExists => CreateDatabaseFileError::AlreadyExists,
            _ => CreateDatabaseFileError::Io(error),
        })?;
    driver.initialize_new_database_file(path).map_err(|error| {
        let _ = std::fs::remove_file(path);
        CreateDatabaseFileError::Initialize(error)
    })
}

#[cfg(test)]
#[path = "database_file_tests.rs"]
mod tests;
