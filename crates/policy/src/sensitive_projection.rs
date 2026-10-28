use sqlparser::ast::{Expr, Query, SelectItem, SetExpr, Statement, TableFactor};
#[cfg(test)]
use sqlparser::parser::Parser;

#[cfg(test)]
use crate::classify::dialect_for;
use crate::mask::SensitivePatterns;

#[path = "projection_sources.rs"]
mod projection_sources;

#[cfg(test)]
pub fn sensitive_projection(sql: &str, driver_id: &str, patterns: &[String], output_columns: usize) -> Vec<bool> {
    let dialect = dialect_for(driver_id);
    let trimmed = sql.trim();
    let Ok(statements) = Parser::parse_sql(dialect.as_ref(), trimmed) else {
        return vec![true; output_columns];
    };
    projection_from_statements(&statements, &SensitivePatterns::new(patterns))
        .filter(|positions| positions.len() == output_columns)
        .unwrap_or_else(|| vec![true; output_columns])
}

pub(crate) fn projection_from_statements(statements: &[Statement], patterns: &SensitivePatterns) -> Option<Vec<bool>> {
    let [Statement::Query(query)] = statements else {
        return None;
    };
    select_projection_sensitivity(query, patterns)
}

fn select_projection_sensitivity(query: &Query, patterns: &SensitivePatterns) -> Option<Vec<bool>> {
    if query.with.is_some() {
        return None;
    }
    let SetExpr::Select(select) = query.body.as_ref() else {
        return None;
    };
    if let [item] = select.projection.as_slice()
        && matches!(item, SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _))
    {
        let [table] = select.from.as_slice() else {
            return None;
        };
        if !table.joins.is_empty() {
            return None;
        }
        let TableFactor::Derived { subquery, .. } = &table.relation else {
            return None;
        };
        return select_projection_sensitivity(subquery, patterns);
    }
    let records = projection_sources::source_records(select)?;
    let mut sensitive = Vec::with_capacity(select.projection.len());
    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr)
            | SelectItem::ExprWithAlias { expr, .. }
            | SelectItem::ExprWithAliases { expr, .. } => {
                sensitive.push(
                    expr_references_sensitive(expr, patterns) || projection_sources::references_record(expr, &records),
                );
            }
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => return None,
        }
    }
    Some(sensitive)
}

fn expr_references_sensitive(expr: &Expr, patterns: &SensitivePatterns) -> bool {
    let rendered = expr.to_string();
    let lowered = rendered.to_ascii_lowercase();
    record_serialization_constructor(&lowered) || text_identifiers(&rendered).any(|ident| patterns.matches(ident))
}

fn record_serialization_constructor(lowered: &str) -> bool {
    for name in [
        "row_to_json(",
        "to_json(",
        "to_jsonb(",
        "json_agg(",
        "jsonb_agg(",
        "json_object_agg(",
        "jsonb_object_agg(",
        "json_build_object(",
        "jsonb_build_object(",
        "json_object(",
        "jsonb_object(",
        "table_to_xml(",
        "table_to_xml_and_xmlschema(",
        "schema_to_xml(",
        "schema_to_xml_and_xmlschema(",
        "database_to_xml(",
        "database_to_xml_and_xmlschema(",
        "query_to_xml(",
        "query_to_xml_and_xmlschema(",
        "cursor_to_xml(",
        "cursor_to_xml_and_xmlschema(",
    ] {
        if let Some(function_name) = name.strip_suffix('(')
            && contains_function_call(lowered, function_name)
        {
            return true;
        }
    }
    false
}

fn contains_function_call(sql: &str, function_name: &str) -> bool {
    sql.match_indices(function_name).any(|(start, _)| {
        let preceding_boundary = sql[..start]
            .chars()
            .next_back()
            .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_');
        let after_name = &sql[start + function_name.len()..];
        let after_name = after_name
            .trim_start()
            .strip_prefix('"')
            .unwrap_or(after_name.trim_start());
        preceding_boundary && after_name.trim_start().starts_with('(')
    })
}

fn text_identifiers(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
#[path = "sensitive_projection_tests.rs"]
mod tests;
