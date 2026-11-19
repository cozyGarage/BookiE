use bookie_core::sql_dialect::{IdentError, quote_ident, validate_ident};

#[derive(Debug, PartialEq, Eq)]
pub enum SchemaContextError {
    Unsupported(String),
    Invalid(IdentError),
}

pub fn supports_schema_context(driver_id: &str) -> bool {
    matches!(driver_id, "postgres" | "mysql")
}

pub fn schema_context_statement(driver_id: &str, schema: &str) -> Result<String, SchemaContextError> {
    if !supports_schema_context(driver_id) {
        return Err(SchemaContextError::Unsupported(driver_id.to_string()));
    }
    validate_ident(schema).map_err(SchemaContextError::Invalid)?;
    let quoted = quote_ident(driver_id, schema);
    Ok(match driver_id {
        "postgres" => format!("SET search_path TO {quoted}"),
        _ => format!("USE {quoted}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_sets_the_search_path_with_a_quoted_schema() {
        assert_eq!(
            schema_context_statement("postgres", "billing").as_deref(),
            Ok("SET search_path TO \"billing\"")
        );
        assert_eq!(
            schema_context_statement("postgres", "odd\"name").as_deref(),
            Ok("SET search_path TO \"odd\"\"name\"")
        );
    }

    #[test]
    fn mysql_and_mariadb_switch_database_with_use() {
        assert_eq!(
            schema_context_statement("mysql", "app db").as_deref(),
            Ok("USE `app db`")
        );
        assert_eq!(schema_context_statement("mysql", "a`b").as_deref(), Ok("USE `a``b`"));
    }

    #[test]
    fn an_engine_without_a_session_schema_refuses_instead_of_guessing() {
        for driver in ["sqlite", "mssql", "clickhouse", "duckdb", "redis"] {
            assert_eq!(
                schema_context_statement(driver, "s"),
                Err(SchemaContextError::Unsupported(driver.into())),
                "{driver}"
            );
            assert!(!supports_schema_context(driver));
        }
    }

    #[test]
    fn an_empty_or_control_character_schema_is_refused_before_any_sql_is_built() {
        assert_eq!(
            schema_context_statement("postgres", "  "),
            Err(SchemaContextError::Invalid(IdentError::Empty))
        );
        assert_eq!(
            schema_context_statement("postgres", "a\nb"),
            Err(SchemaContextError::Invalid(IdentError::ControlCharacter))
        );
    }
}
