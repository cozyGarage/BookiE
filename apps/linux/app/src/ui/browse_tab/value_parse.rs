//! GTK-side entry points for value parsing. The parser lives in `bookie_app_services::value_input`; these
//! wrappers take the driver's enum input style and MongoDB Decimal128 canonicalisation through capabilities.

pub(super) use bookie_app_services::value_input::*;

use bookie_core::{ColumnInfo, EnumInputStyle, Value};

pub(super) fn input_error_message(error: ValueInputError) -> String {
    bookie_app_services::edit_error_text::value_input_message(&error)
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
