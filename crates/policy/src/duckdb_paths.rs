use std::ops::ControlFlow;

use sqlparser::ast::{Ident, ObjectName, ObjectNamePart, Statement, TableFactor, Visit, Visitor};

const SAFE_TABLE_FUNCTIONS: [&str; 22] = [
    "range",
    "generate_series",
    "generate_subscripts",
    "unnest",
    "json_each",
    "json_tree",
    "repeat_row",
    "duckdb_tables",
    "duckdb_columns",
    "duckdb_views",
    "duckdb_schemas",
    "duckdb_indexes",
    "duckdb_constraints",
    "duckdb_sequences",
    "duckdb_types",
    "duckdb_functions",
    "duckdb_keywords",
    "duckdb_dependencies",
    "duckdb_comments",
    "pragma_table_info",
    "pragma_storage_info",
    "pragma_version",
];

const SENSITIVE_SOURCES: [&str; 6] = [
    "duckdb_secrets",
    "duckdb_settings",
    "duckdb_databases",
    "duckdb_extensions",
    "duckdb_temporary_files",
    "pragma_database_list",
];

const BUILTIN_SCHEMAS: [&str; 2] = ["main", "system"];

struct SourceFinder;

pub(crate) fn reads_a_path(statements: &[Statement]) -> bool {
    statements
        .iter()
        .any(|statement| statement.visit(&mut SourceFinder).is_break())
}

impl Visitor for SourceFinder {
    type Break = ();

    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<()> {
        match statement {
            Statement::ExplainTable { table_name, .. } if names_a_path(table_name) => ControlFlow::Break(()),
            _ => ControlFlow::Continue(()),
        }
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<()> {
        let foreign = match factor {
            TableFactor::Table {
                name, args: Some(_), ..
            }
            | TableFactor::Function { name, .. } => !is_safe_table_function(name),
            TableFactor::Table { name, args: None, .. } => names_a_path(name) || names_a_sensitive_source(name),
            TableFactor::Derived { .. }
            | TableFactor::NestedJoin { .. }
            | TableFactor::Pivot { .. }
            | TableFactor::Unpivot { .. }
            | TableFactor::UnpivotExpr { .. }
            | TableFactor::UNNEST { .. }
            | TableFactor::MatchRecognize { .. } => false,
            _ => true,
        };
        if foreign {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

fn identifiers(name: &ObjectName) -> impl Iterator<Item = &Ident> {
    name.0.iter().filter_map(|part| match part {
        ObjectNamePart::Identifier(ident) => Some(ident),
        _ => None,
    })
}

fn is_safe_table_function(name: &ObjectName) -> bool {
    let parts: Vec<&Ident> = identifiers(name).collect();
    let qualifier_is_builtin = match parts.as_slice() {
        [_] => true,
        [schema, _] => BUILTIN_SCHEMAS.contains(&schema.value.to_ascii_lowercase().as_str()),
        _ => false,
    };
    qualifier_is_builtin
        && parts
            .last()
            .is_some_and(|ident| SAFE_TABLE_FUNCTIONS.contains(&ident.value.to_ascii_lowercase().as_str()))
}

fn names_a_path(name: &ObjectName) -> bool {
    identifiers(name).last().is_some_and(|ident| match ident.quote_style {
        Some('\'') => true,
        Some(_) => looks_like_a_path(&ident.value),
        None => false,
    })
}

fn names_a_sensitive_source(name: &ObjectName) -> bool {
    identifiers(name)
        .last()
        .is_some_and(|ident| SENSITIVE_SOURCES.contains(&ident.value.to_ascii_lowercase().as_str()))
}

fn looks_like_a_path(name: &str) -> bool {
    name.contains(['/', '\\', '.', '*', '?'])
}
