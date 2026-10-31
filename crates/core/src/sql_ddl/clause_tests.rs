use super::BuildDdlError;
use super::types::{validate_safe_default, validate_safe_type};

#[test]
fn a_type_cannot_carry_a_second_clause() {
    for ty in [
        "int, DROP COLUMN secret",
        "int, ADD COLUMN b int",
        "varchar(10",
        "varchar(10))",
        "text 'unterminated",
    ] {
        assert!(
            matches!(validate_safe_type(ty), Err(BuildDdlError::UnsafeType(_))),
            "accepted: {ty}"
        );
    }
}

#[test]
fn a_default_cannot_carry_a_second_clause() {
    for default in [
        "0, DROP COLUMN x",
        "now(), ADD COLUMN b int",
        "(1+2",
        "1+2)",
        "'unterminated",
    ] {
        assert!(
            matches!(validate_safe_default(default), Err(BuildDdlError::UnsafeDefault(_))),
            "accepted: {default}"
        );
    }
}

#[test]
fn commas_inside_parentheses_and_quotes_are_still_allowed() {
    for ty in ["DECIMAL(10, 2)", "ENUM('a,b','c')", "Tuple(Int8, String)"] {
        assert!(validate_safe_type(ty).is_ok(), "rejected: {ty}");
    }
    for default in ["'a,b'", "coalesce(a, b)", "'O''Brien, J'"] {
        assert!(validate_safe_default(default).is_ok(), "rejected: {default}");
    }
}
