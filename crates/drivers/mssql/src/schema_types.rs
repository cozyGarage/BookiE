//! Column metadata and default-expression helpers.

use super::*;

pub(crate) fn row_to_column_info(row: &[Value]) -> ColumnInfo {
    let type_name = as_text(row.get(1)).unwrap_or_default();
    let max_length = as_i64(row.get(2)).unwrap_or(0);
    let precision = as_i64(row.get(3)).unwrap_or(0);
    let scale = as_i64(row.get(4)).unwrap_or(0);
    let is_identity = as_bool(row.get(6)).unwrap_or(false);
    let default_raw = as_text(row.get(8));
    ColumnInfo {
        name: as_text(row.first()).unwrap_or_default(),
        data_type: format_mssql_type(&type_name, max_length, precision, scale),
        nullable: as_bool(row.get(5)).unwrap_or(true),
        primary_key: as_bool(row.get(9)).unwrap_or(false),
        is_auto_increment: is_identity,
        // IDENTITY columns carry an internal seed/increment, not a user
        // DEFAULT — suppress so the inline-insert UI treats them as auto.
        default_value: if is_identity {
            None
        } else {
            default_raw.map(|d| normalize_mssql_default(&d))
        },
        is_generated: as_bool(row.get(7)).unwrap_or(false),
        comment: as_text(row.get(10)).filter(|c| !c.is_empty()),
        collation: as_text(row.get(11)).filter(|c| !c.is_empty()),
        enum_type: None,
        domain_type: None,
    }
}

/// Rebuild a user-facing type string from `sys.types` metadata. `nvarchar`
/// / `nchar` store `max_length` in bytes (two per character); `-1` is the
/// `(max)` sentinel. Numeric types carry precision/scale.
pub(crate) fn format_mssql_type(type_name: &str, max_length: i64, precision: i64, scale: i64) -> String {
    match type_name.to_ascii_lowercase().as_str() {
        "varchar" | "char" | "varbinary" | "binary" => {
            if max_length < 0 {
                format!("{type_name}(max)")
            } else {
                format!("{type_name}({max_length})")
            }
        }
        "nvarchar" | "nchar" => {
            if max_length < 0 {
                format!("{type_name}(max)")
            } else {
                format!("{type_name}({})", max_length / 2)
            }
        }
        "decimal" | "numeric" => format!("{type_name}({precision},{scale})"),
        // The app parser needs the declared precision to refuse datetimeoffset
        // edits that SQL Server would round during implicit text conversion.
        "datetimeoffset" => format!("{type_name}({scale})"),
        _ => type_name.to_string(),
    }
}

// sys.default_constraints.definition wraps the expression in one or two
// pairs of parentheses (`((0))`, `('pending')`); they are peeled so the
// default reads as the DEFAULT clause that created it.
pub(crate) fn normalize_mssql_default(raw: &str) -> String {
    let mut s = raw.trim();
    while outer_parens_wrap(s) {
        s = s[1..s.len() - 1].trim();
    }
    s.to_string()
}

pub(crate) fn outer_parens_wrap(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'(' || bytes[bytes.len() - 1] != b')' {
        return false;
    }
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                // A close that returns to depth 0 before the final byte
                // means the leading `(` does not wrap the whole string
                // (e.g. `(a)+(b)`); don't strip.
                if depth == 0 && i != bytes.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Map SQL Server's `*_referential_action_desc` text to the canonical SQL
/// keyword the FK/DDL layer expects. `NO_ACTION` returns `None` so the DDL
/// builder omits the redundant clause.
pub(crate) fn map_referential_action(desc: &str) -> Option<String> {
    match desc.to_ascii_uppercase().as_str() {
        "CASCADE" => Some("CASCADE".into()),
        "SET_NULL" => Some("SET NULL".into()),
        "SET_DEFAULT" => Some("SET DEFAULT".into()),
        _ => None,
    }
}
