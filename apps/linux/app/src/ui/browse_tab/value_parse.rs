//! GTK-side entry points for value parsing. The parser lives in `bookie_app_services::value_input`; these
//! wrappers take the driver's enum input style and MongoDB Decimal128 canonicalisation through capabilities.

pub(super) use bookie_app_services::value_input::*;

use bookie_core::{ColumnInfo, EnumInputStyle, Value};

pub(super) fn input_error_message(error: ValueInputError) -> String {
    use ValueInputError as Error;
    match error {
        Error::FieldRequired => bookie_app_services::tr!("Field is required"),
        Error::InvalidEnumMetadata => bookie_app_services::tr!("Could not safely read ENUM or SET members"),
        Error::UndeclaredEnumMember => bookie_app_services::tr!("Value is not a declared ENUM or SET member"),
        Error::InvalidBoolean => bookie_app_services::tr!("Invalid boolean. Use true/false, yes/no, or 1/0."),
        Error::InvalidInteger => bookie_app_services::tr!("Invalid integer"),
        Error::InvalidNumber => bookie_app_services::tr!("Invalid number"),
        Error::InvalidDecimal => bookie_app_services::tr!("Invalid decimal"),
        Error::InvalidUuid => bookie_app_services::tr!("Invalid UUID. Expected 8-4-4-4-12 hex digits."),
        Error::InvalidJson(error) => bookie_app_services::tr!("Invalid JSON: {error}").replace("{error}", &error),
        Error::InvalidTimestamp => {
            bookie_app_services::tr!("Invalid timestamp. Use ISO 8601, e.g. 2024-01-15T14:30:00Z.")
        }
        Error::InvalidTimestampWithTimeZone => bookie_app_services::tr!("Invalid timestamp with time zone."),
        Error::InvalidDateTime => bookie_app_services::tr!("Invalid datetime. Use YYYY-MM-DD HH:MM:SS."),
        Error::InvalidDateAndTime => bookie_app_services::tr!("Invalid date and time."),
        Error::InvalidDate => bookie_app_services::tr!("Invalid date. Use YYYY-MM-DD."),
        Error::InvalidTime => bookie_app_services::tr!("Invalid time. Use HH:MM:SS."),
        Error::InvalidSqlServerDateTimeOffsetColumnScale => {
            bookie_app_services::tr!("Invalid SQL Server datetimeoffset column scale")
        }
        Error::InvalidSqlServerDateTimeOffset => bookie_app_services::tr!("Invalid SQL Server datetimeoffset"),
        Error::SqlServerDateTimeOffsetOutOfRange => {
            bookie_app_services::tr!("SQL Server datetimeoffset value exceeds the column precision or offset range")
        }
        Error::MySqlSpatialValueNotEditable => {
            bookie_app_services::tr!("MySQL spatial values cannot be edited losslessly")
        }
        Error::MySqlIntegerOutOfRange => {
            bookie_app_services::tr!("Integer is outside the supported MySQL column range")
        }
        Error::MongoInt32OutOfRange => {
            bookie_app_services::tr!("Integer is outside the supported MongoDB Int32 range")
        }
        Error::MongoDatePrecision => bookie_app_services::tr!("MongoDB BSON dates support millisecond precision only"),
        Error::DuckDbTimestampTzPrecision => {
            bookie_app_services::tr!("DuckDB TIMESTAMPTZ supports microsecond precision only")
        }
        Error::DuckDbTemporalPrecision { kind } => {
            bookie_app_services::tr!("DuckDB {kind} precision cannot represent this value exactly")
                .replace("{kind}", kind)
        }
        Error::BitWidthOutOfRange => bookie_app_services::tr!("Integer is outside the supported BIT range"),
    }
}

#[cfg(test)]
pub(super) fn parse_input_for_column(text: &str, col: Option<&ColumnInfo>) -> Result<Value, String> {
    bookie_app_services::value_input::parse_input_for_column(text, col).map_err(input_error_message)
}

#[cfg(test)]
pub(super) fn parse_input_for_driver_with_style(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Result<Value, String> {
    bookie_app_services::value_input::parse_input_for_driver_with_style(text, col, hooks).map_err(input_error_message)
}

#[cfg(test)]
pub(super) fn parse_input_for_grid_cell_with_style(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
    current_value: Option<&Value>,
) -> Result<Value, String> {
    bookie_app_services::value_input::parse_input_for_grid_cell_with_style(text, col, hooks, current_value)
        .map_err(input_error_message)
}

pub(super) fn parse_grid_cell_input_with_style(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
    current_value: Option<&Value>,
) -> Result<Value, String> {
    bookie_app_services::value_input::parse_grid_cell_input_with_style(text, col, hooks, current_value)
        .map_err(input_error_message)
}

pub(super) fn parse_pasted_cell_with_style(
    text: Option<&str>,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
    current_value: Option<&Value>,
) -> Result<Value, String> {
    bookie_app_services::value_input::parse_pasted_cell_with_style(text, col, hooks, current_value)
        .map_err(input_error_message)
}

/// The hooks for one driver, from the driver registry.
/// The hooks for one driver, from the driver registry.
pub(crate) fn hooks_for(driver_id: &str) -> ParseHooks {
    use std::sync::OnceLock;

    static REGISTRY: OnceLock<bookie_core::DriverRegistry> = OnceLock::new();
    REGISTRY.get_or_init(crate::build_registry).get(driver_id).map_or_else(
        || ParseHooks::without_driver(EnumInputStyle::RawValue),
        |driver| ParseHooks {
            enum_input_style: driver.capabilities().enum_input_style,
            capabilities: driver.value_input_capabilities(),
        },
    )
}

#[cfg(test)]
pub(super) fn parse_input_for_driver(text: &str, col: Option<&ColumnInfo>, driver_id: &str) -> Result<Value, String> {
    parse_input_for_driver_with_style(text, col, &hooks_for(driver_id))
}

#[cfg(test)]
pub(super) fn parse_input_for_grid_cell(
    text: &str,
    col: Option<&ColumnInfo>,
    driver_id: &str,
    current_value: Option<&Value>,
) -> Result<Value, String> {
    parse_input_for_grid_cell_with_style(text, col, &hooks_for(driver_id), current_value)
}

pub(in crate::ui) fn parse_grid_cell_input(
    text: &str,
    col: Option<&ColumnInfo>,
    driver_id: &str,
    current_value: Option<&Value>,
) -> Result<Value, String> {
    parse_grid_cell_input_with_style(text, col, &hooks_for(driver_id), current_value)
}

#[cfg(test)]
pub(super) fn parse_pasted_cell(
    text: Option<&str>,
    col: Option<&ColumnInfo>,
    driver_id: &str,
    current_value: Option<&Value>,
) -> Result<Value, String> {
    parse_pasted_cell_with_style(text, col, &hooks_for(driver_id), current_value)
}

#[cfg(test)]
#[path = "../../../../../../crates/core/tests/support/parser_contract.rs"]
mod parser_contract;

#[cfg(test)]
#[path = "../../../tests/support/value_parse.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/support/sqlite_any_contract.rs"]
mod sqlite_any_contract;

#[cfg(test)]
#[path = "../../../tests/support/empty_text_contract.rs"]
mod empty_text_contract;

#[cfg(test)]
#[path = "../../../tests/support/duckdb_enum_contract.rs"]
mod duckdb_enum_contract;

#[cfg(test)]
#[path = "../../../tests/support/clickhouse_enum_contract.rs"]
mod clickhouse_enum_contract;

#[cfg(test)]
#[path = "../../../tests/support/mysql_integer_contract.rs"]
mod mysql_integer_contract;

#[cfg(test)]
#[path = "../../../tests/support/mysql_enum_set_contract.rs"]
mod mysql_enum_set_contract;

#[cfg(test)]
#[path = "../../../tests/support/mysql_temporal_contract.rs"]
mod mysql_temporal_contract;

#[cfg(test)]
#[path = "../../../tests/support/mssql_legacy_datetime_contract.rs"]
mod mssql_legacy_datetime_contract;

#[cfg(test)]
#[path = "../../../tests/support/mssql_datetimeoffset_contract.rs"]
mod mssql_datetimeoffset_contract;

#[cfg(test)]
#[path = "../../../tests/support/mssql_max_value_contract.rs"]
mod mssql_max_value_contract;

#[cfg(test)]
#[path = "../../../tests/support/mssql_server_owned_columns.rs"]
mod mssql_server_owned_columns;

#[cfg(test)]
#[path = "../../../tests/support/postgres_identity_csv_import.rs"]
mod postgres_identity_csv_import;

#[cfg(test)]
#[path = "../../../tests/support/mysql_spatial_contract.rs"]
mod mysql_spatial_contract;

#[cfg(test)]
#[path = "../../../tests/support/postgres_array_contract.rs"]
mod postgres_array_contract;

#[cfg(test)]
#[path = "../../../tests/support/postgres_bit_array_grid.rs"]
mod postgres_bit_array_grid;

#[cfg(test)]
#[path = "../../../tests/support/postgres_citext_array_grid.rs"]
mod postgres_citext_array_grid;

#[cfg(test)]
#[path = "../../../tests/support/mongodb_nested_edit_contract.rs"]
mod mongodb_nested_edit_contract;

#[cfg(test)]
#[path = "../../../tests/support/mongodb_integer_width.rs"]
mod mongodb_integer_width;
