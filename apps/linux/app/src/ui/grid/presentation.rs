use tablepro_core::{ColumnInfo, Value};

use super::display::value_is_inline_editable;
use super::types::{is_bool_type, is_bytes_type};

pub(super) fn column_is_editable(column: &ColumnInfo) -> bool {
    !column.primary_key
        && !column.is_generated
        && !column.is_auto_increment
        && !is_bytes_type(&column.data_type)
        && !column.data_type.eq_ignore_ascii_case("mixed")
}

/// Declared affinity is not a guarantee about a row's runtime value.
/// A checkbox must never turn an unrepresentable value into a boolean.
pub(crate) fn cell_allows_inline_edit(column: &ColumnInfo, value: &Value) -> bool {
    column_is_editable(column)
        && value_is_inline_editable(value)
        && (!is_bool_type(&column.data_type)
            || matches!(value, Value::Bool(_) | Value::Null)
            || (column.data_type.eq_ignore_ascii_case("bit(1)") && matches!(value, Value::Int(0 | 1))))
}

pub(crate) fn cell_allows_set_null(column: &ColumnInfo, value: &Value) -> bool {
    column.nullable && cell_allows_inline_edit(column, value)
}

#[cfg(test)]
#[path = "presentation_tests.rs"]
mod tests;
