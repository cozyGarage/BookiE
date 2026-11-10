use bookie_core::sql_ddl::{BuildDdlError, build_restart_sequence};
use bookie_core::{Connection, DriverError, OperationControl};

#[derive(Debug, thiserror::Error)]
pub enum SetSequenceError {
    #[error("enter a whole number")]
    NotANumber,
    #[error(transparent)]
    Statement(#[from] BuildDdlError),
    #[error(transparent)]
    Driver(#[from] DriverError),
}

pub fn parse_sequence_value(text: &str) -> Result<i64, SetSequenceError> {
    text.trim().parse().map_err(|_| SetSequenceError::NotANumber)
}

pub async fn set_sequence_value(
    connection: &dyn Connection,
    driver_id: &str,
    schema: Option<&str>,
    sequence: &str,
    value_text: &str,
    control: &OperationControl,
) -> Result<(), SetSequenceError> {
    let value = parse_sequence_value(value_text)?;
    let sql = build_restart_sequence(driver_id, schema, sequence, value)?;
    connection.execute_controlled(&sql, control).await?;
    Ok(())
}

#[cfg(test)]
#[path = "sequence_value_tests.rs"]
mod tests;
