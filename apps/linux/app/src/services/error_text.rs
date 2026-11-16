use bookie_core::DriverError;
use bookie_core::sql_dialect::BuildSqlError;

pub fn build_sql_message(error: &BuildSqlError) -> String {
    match error {
        BuildSqlError::NoPrimaryKey => {
            bookie_app_services::tr!("This table has no primary key. Use the modal Edit dialog instead.")
        }
        BuildSqlError::NothingToUpdate => bookie_app_services::tr!("No changes to save."),
        BuildSqlError::LengthMismatch { expected, got } => {
            bookie_app_services::tr!("Internal column count mismatch (expected {expected}, got {got}).")
                .replace("{expected}", &expected.to_string())
                .replace("{got}", &got.to_string())
        }
        BuildSqlError::UnrepresentableValue { column } => {
            bookie_app_services::tr!("Column {column} contains a value that cannot be copied into a SQL statement.")
                .replace("{column}", column)
        }
        BuildSqlError::UnsupportedDriver(driver) => {
            bookie_app_services::tr!("Optimistic grid updates are not supported for {driver}.")
                .replace("{driver}", driver)
        }
        BuildSqlError::DefaultUnsupported(driver) => {
            bookie_app_services::tr!("{driver} cannot set a column to its default value.").replace("{driver}", driver)
        }
        BuildSqlError::EmptyExpression => bookie_app_services::tr!("Type an expression first."),
        BuildSqlError::StaleColumns => bookie_app_services::tr!(
            "This table's columns changed after you started editing (for example, in a Structure tab). \
             Reload the page and reapply your changes."
        ),
    }
}

pub fn driver_message(error: &DriverError) -> String {
    let presentation = bookie_app_services::driver_error_presentation::classify(error);
    bookie_app_services::driver_failure_message::driver_failure_message(&presentation)
}

pub fn open_failure_message(failure: &bookie_app_services::open_failure::OpenFailure) -> String {
    bookie_app_services::driver_failure_message::open_failure_message(failure)
}

pub fn create_database_file_message(
    error: &bookie_app_services::database_file::CreateDatabaseFileError,
    path: &std::path::Path,
) -> String {
    use bookie_app_services::database_file::CreateDatabaseFileError;
    let path = path.display().to_string();
    match error {
        CreateDatabaseFileError::AlreadyExists => bookie_app_services::tr!(
            "{path} already exists and was not replaced. Use Choose an existing database file to open it, or pick a new name."
        )
        .replace("{path}", &path),
        CreateDatabaseFileError::Initialize(source) => {
            bookie_app_services::tr!("Could not create {path}: {detail}. Check the folder and its permissions.")
                .replace("{path}", &path)
                .replace("{detail}", &driver_message(source))
        }
        CreateDatabaseFileError::Io(source) => {
            bookie_app_services::tr!("Could not create {path}: {detail}. Check the folder and its permissions.")
                .replace("{path}", &path)
                .replace("{detail}", &source.to_string())
        }
    }
}

pub fn transport_message(error: bookie_transport::TransportError) -> String {
    open_failure_message(&bookie_app_services::open_failure::OpenFailure::from(error))
}

#[cfg(test)]
#[path = "error_text_tests.rs"]
mod tests;
