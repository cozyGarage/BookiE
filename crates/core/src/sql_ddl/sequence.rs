use super::types::{BuildDdlError, qualified_table, validate_table};

pub fn build_restart_sequence(
    driver_id: &str,
    schema: Option<&str>,
    sequence: &str,
    next_value: i64,
) -> Result<String, BuildDdlError> {
    validate_table(sequence)?;
    match driver_id {
        "postgres" => Ok(format!(
            "ALTER SEQUENCE {} RESTART WITH {next_value}",
            qualified_table(driver_id, schema, sequence)
        )),
        other => Err(BuildDdlError::UnsupportedDriver(other.to_string())),
    }
}
