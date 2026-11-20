use crate::set_cells::SetCellsError;
use crate::value_input::ValueInputError;

pub fn value_input_message(error: &ValueInputError) -> String {
    use ValueInputError as Error;
    match error.clone() {
        Error::FieldRequired => crate::tr!("Field is required"),
        Error::InvalidEnumMetadata => crate::tr!("Could not safely read ENUM or SET members"),
        Error::UndeclaredEnumMember => crate::tr!("Value is not a declared ENUM or SET member"),
        Error::InvalidBoolean => crate::tr!("Invalid boolean. Use true/false, yes/no, or 1/0."),
        Error::InvalidInteger => crate::tr!("Invalid integer"),
        Error::InvalidNumber => crate::tr!("Invalid number"),
        Error::InvalidDecimal => crate::tr!("Invalid decimal"),
        Error::InvalidUuid => crate::tr!("Invalid UUID. Expected 8-4-4-4-12 hex digits."),
        Error::InvalidJson(error) => crate::tr!("Invalid JSON: {error}").replace("{error}", &error),
        Error::InvalidTimestamp => {
            crate::tr!("Invalid timestamp. Use ISO 8601, e.g. 2024-01-15T14:30:00Z.")
        }
        Error::InvalidTimestampWithTimeZone => crate::tr!("Invalid timestamp with time zone."),
        Error::InvalidDateTime => crate::tr!("Invalid datetime. Use YYYY-MM-DD HH:MM:SS."),
        Error::InvalidDateAndTime => crate::tr!("Invalid date and time."),
        Error::InvalidDate => crate::tr!("Invalid date. Use YYYY-MM-DD."),
        Error::InvalidTime => crate::tr!("Invalid time. Use HH:MM:SS."),
        Error::InvalidSqlServerDateTimeOffsetColumnScale => {
            crate::tr!("Invalid SQL Server datetimeoffset column scale")
        }
        Error::InvalidSqlServerDateTimeOffset => crate::tr!("Invalid SQL Server datetimeoffset"),
        Error::SqlServerDateTimeOffsetOutOfRange => {
            crate::tr!("SQL Server datetimeoffset value exceeds the column precision or offset range")
        }
        Error::MySqlSpatialValueNotEditable => {
            crate::tr!("MySQL spatial values cannot be edited losslessly")
        }
        Error::MySqlIntegerOutOfRange => {
            crate::tr!("Integer is outside the supported MySQL column range")
        }
        Error::MongoInt32OutOfRange => {
            crate::tr!("Integer is outside the supported MongoDB Int32 range")
        }
        Error::MongoDatePrecision => crate::tr!("MongoDB BSON dates support millisecond precision only"),
        Error::DuckDbTimestampTzPrecision => {
            crate::tr!("DuckDB TIMESTAMPTZ supports microsecond precision only")
        }
        Error::DuckDbTemporalPrecision { kind } => {
            crate::tr!("DuckDB {kind} precision cannot represent this value exactly").replace("{kind}", kind)
        }
        Error::BitWidthOutOfRange => crate::tr!("Integer is outside the supported BIT range"),
    }
}

pub fn set_cells_message(error: &SetCellsError) -> String {
    match error {
        SetCellsError::TableNotEditable => {
            crate::tr!("This table cannot be edited, so no cells were set.")
        }
        SetCellsError::NothingSelected => crate::tr!("No cells are selected."),
        SetCellsError::TooManyCells => crate::tr!("That is too many cells to set at once"),
        SetCellsError::ReadOnlyColumn { column } => {
            crate::tr!("{column} is a key, generated or identity column and cannot be set. No cells were changed.")
                .replace("{column}", column)
        }
        SetCellsError::NotNullable { column } => {
            crate::tr!("{column} does not accept NULL. No cells were changed.").replace("{column}", column)
        }
        SetCellsError::NotEditable { column, position } => {
            crate::tr!("{column} on row {row} can't be edited, so no cells were changed.")
                .replace("{column}", column)
                .replace("{row}", &(position + 1).to_string())
        }
        SetCellsError::InvalidValue { column, error } => crate::tr!("{column}: {message}. No cells were changed.")
            .replace("{column}", column)
            .replace("{message}", &value_input_message(error)),
    }
}

#[cfg(test)]
#[path = "edit_error_text_tests.rs"]
mod tests;
