use tablepro_core::{ColumnInfo, Value};

use super::types::{is_bool_type, is_bytes_type};

pub(super) fn column_is_editable(column: &ColumnInfo) -> bool {
    !column.primary_key && !column.is_generated && !column.is_auto_increment && !is_bytes_type(&column.data_type)
}

/// Declared affinity is not a guarantee about a row's runtime value.
/// A checkbox must never turn an unrepresentable value into a boolean.
pub(crate) fn cell_allows_inline_edit(column: &ColumnInfo, value: &Value) -> bool {
    column_is_editable(column)
        && !matches!(value, Value::Bytes(_))
        && (!is_bool_type(&column.data_type) || matches!(value, Value::Bool(_) | Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_values_and_column_ownership_both_gate_edits() {
        let mut column = ColumnInfo {
            name: "value".into(),
            data_type: "BOOLEAN".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            is_generated: false,
            default_value: None,
        };
        for value in [Value::Bytes(vec![1]), Value::Text("true".into()), Value::Int(2)] {
            assert!(!cell_allows_inline_edit(&column, &value));
        }
        for value in [Value::Null, Value::Bool(false), Value::Bool(true)] {
            assert!(cell_allows_inline_edit(&column, &value));
        }
        column.primary_key = true;
        assert!(!cell_allows_inline_edit(&column, &Value::Bool(true)));
    }
}
