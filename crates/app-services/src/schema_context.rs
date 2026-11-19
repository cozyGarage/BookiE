use bookie_core::sql_dialect::{IdentError, quote_ident, validate_ident};

#[derive(Debug, PartialEq, Eq)]
pub enum SchemaContextError {
    Unsupported(String),
    Invalid(IdentError),
    NeedsSession,
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

pub fn schema_context_for_run(
    driver_id: &str,
    schema: &str,
    session_pinned: bool,
) -> Result<Option<String>, SchemaContextError> {
    let schema = schema.trim();
    if schema.is_empty() {
        return Ok(None);
    }
    if !supports_schema_context(driver_id) {
        return Err(SchemaContextError::Unsupported(driver_id.to_string()));
    }
    if !session_pinned {
        return Err(SchemaContextError::NeedsSession);
    }
    schema_context_statement(driver_id, schema).map(Some)
}

pub fn schema_context_label(session_label: &str, schema: &str, pinned: bool) -> String {
    let schema = schema.trim();
    if !pinned || schema.is_empty() {
        return session_label.to_string();
    }
    let mut shown: String = schema.chars().take(32).collect();
    if schema.chars().count() > shown.chars().count() {
        shown.push('…');
    }
    format!("{session_label} · {shown}")
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

    #[test]
    fn a_blank_schema_keeps_the_session_default_even_when_no_session_is_pinned() {
        assert_eq!(schema_context_for_run("postgres", "  ", false), Ok(None));
        assert_eq!(schema_context_for_run("sqlite", "", false), Ok(None));
    }

    #[test]
    fn a_named_schema_is_applied_only_on_a_pinned_session() {
        assert_eq!(
            schema_context_for_run("postgres", " billing ", true)
                .as_ref()
                .map(|sql| sql.as_deref()),
            Ok(Some("SET search_path TO \"billing\""))
        );
        assert_eq!(
            schema_context_for_run("mysql", "app", false),
            Err(SchemaContextError::NeedsSession)
        );
        assert_eq!(
            schema_context_for_run("sqlite", "main", true),
            Err(SchemaContextError::Unsupported("sqlite".into()))
        );
        assert_eq!(
            schema_context_for_run("postgres", "a\nb", true),
            Err(SchemaContextError::Invalid(IdentError::ControlCharacter))
        );
    }

    #[test]
    fn the_session_label_shows_the_pinned_schema_and_leaves_the_default_alone() {
        assert_eq!(schema_context_label("Session", "  ", true), "Session");
        assert_eq!(schema_context_label("Session", "billing", false), "Session");
        assert_eq!(schema_context_label("Session", "billing", true), "Session · billing");
        let long = "s".repeat(40);
        let label = schema_context_label("Session", &long, true);
        assert!(label.starts_with("Session · "));
        assert!(label.ends_with('…'));
        assert!(label.chars().count() < "Session · ".chars().count() + long.chars().count());
    }
}
