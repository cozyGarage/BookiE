use std::ops::ControlFlow;

use sqlparser::ast::{Ident, ObjectName, ObjectNamePart, Statement, TableFactor, Visit, Visitor};

const SAFE_TABLE_FUNCTIONS: [&str; 7] = [
    "range",
    "generate_series",
    "generate_subscripts",
    "unnest",
    "json_each",
    "json_tree",
    "repeat_row",
];

const DATA_FILE_EXTENSIONS: [&str; 17] = [
    "csv", "tsv", "parquet", "json", "ndjson", "jsonl", "xlsx", "arrow", "avro", "orc", "txt", "geojson", "db",
    "duckdb", "sqlite", "sqlite3", "xml",
];

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
            TableFactor::Table { name, args: None, .. } => names_a_path(name),
            _ => false,
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
    identifiers(name).last().is_some_and(|ident| {
        let lowered = ident.value.to_ascii_lowercase();
        SAFE_TABLE_FUNCTIONS.contains(&lowered.as_str())
            || lowered.starts_with("duckdb_")
            || lowered.starts_with("pragma_")
    })
}

fn names_a_path(name: &ObjectName) -> bool {
    identifiers(name).any(|ident| match ident.quote_style {
        Some('\'') => true,
        Some(_) => looks_like_a_path(&ident.value),
        None => false,
    })
}

fn looks_like_a_path(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    lowered.contains('/')
        || lowered.contains('\\')
        || lowered.contains('*')
        || lowered.contains('?')
        || lowered
            .split('.')
            .skip(1)
            .any(|segment| DATA_FILE_EXTENSIONS.contains(&segment))
}
