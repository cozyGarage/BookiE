use sqlparser::ast::{DescribeAlias, Expr, Query, SelectItem, SetExpr, Statement, TableFactor};
use sqlparser::parser::Parser;

use crate::classify::dialect_for;
use crate::mask::column_is_sensitive;

pub fn sensitive_projection(sql: &str, driver_id: &str, patterns: &[String], output_columns: usize) -> Vec<bool> {
    let dialect = dialect_for(driver_id);
    let trimmed = sql.trim();
    let statements = Parser::parse_sql(dialect.as_ref(), trimmed);
    match statements {
        Ok(statements) => match statements.as_slice() {
            [Statement::Query(query)] => select_projection_sensitivity(query, patterns)
                .filter(|positions| positions.len() == output_columns)
                .unwrap_or_else(|| vec![true; output_columns]),
            [
                Statement::Explain {
                    describe_alias: DescribeAlias::Explain,
                    analyze: false,
                    statement,
                    ..
                },
            ] if matches!(statement.as_ref(), Statement::Query(_)) => vec![false; output_columns],
            _ => vec![true; output_columns],
        },
        Err(_) => vec![true; output_columns],
    }
}

fn select_projection_sensitivity(query: &Query, patterns: &[String]) -> Option<Vec<bool>> {
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
    let mut sensitive = Vec::with_capacity(select.projection.len());
    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                sensitive.push(expr_references_sensitive(expr, patterns));
            }
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => return None,
        }
    }
    Some(sensitive)
}

fn expr_references_sensitive(expr: &Expr, patterns: &[String]) -> bool {
    let rendered = expr.to_string();
    rendered.to_ascii_lowercase().contains("row_to_json(")
        || text_identifiers(&rendered).any(|ident| column_is_sensitive(ident, patterns))
}

fn text_identifiers(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensitive_patterns() -> Vec<String> {
        crate::mask::DEFAULT_SENSITIVE_PATTERNS
            .iter()
            .map(|p| (*p).to_string())
            .collect()
    }

    #[test]
    fn an_aliased_sensitive_column_is_flagged() {
        let positions = sensitive_projection("SELECT pan AS p FROM cards", "postgres", &sensitive_patterns(), 1);
        assert_eq!(positions, vec![true]);
    }

    #[test]
    fn a_sensitive_column_wrapped_in_an_expression_is_flagged() {
        let positions = sensitive_projection(
            "SELECT substr(pan,1,8) FROM cards",
            "postgres",
            &sensitive_patterns(),
            1,
        );
        assert_eq!(positions, vec![true]);
    }

    #[test]
    fn a_wildcard_over_a_derived_table_sees_through_to_the_inner_alias() {
        let positions = sensitive_projection(
            "SELECT * FROM (SELECT pan AS v FROM cards) t",
            "postgres",
            &sensitive_patterns(),
            1,
        );
        assert_eq!(positions, vec![true]);
    }

    #[test]
    fn an_unrelated_column_is_not_flagged() {
        let positions = sensitive_projection("SELECT amount AS a FROM orders", "postgres", &sensitive_patterns(), 1);
        assert_eq!(positions, vec![false]);
    }

    #[test]
    fn a_plain_explain_of_a_query_keeps_plan_output_visible() {
        let positions = sensitive_projection(
            "EXPLAIN SELECT * FROM cards WHERE pan = '4111111111111111'",
            "postgres",
            &sensitive_patterns(),
            1,
        );
        assert_eq!(positions, vec![false]);
    }

    #[test]
    fn explain_analyze_and_non_query_plans_remain_redacted() {
        for sql in ["EXPLAIN ANALYZE SELECT * FROM cards", "EXPLAIN DELETE FROM cards"] {
            assert_eq!(
                sensitive_projection(sql, "postgres", &sensitive_patterns(), 1),
                vec![true],
                "{sql}"
            );
        }
    }

    #[test]
    fn a_plain_wildcard_over_a_real_table_is_redacted_fail_closed() {
        assert_eq!(
            sensitive_projection("SELECT * FROM cards", "postgres", &sensitive_patterns(), 2),
            vec![true; 2]
        );
    }

    #[test]
    fn a_cte_wildcard_is_redacted_fail_closed() {
        let sql = "WITH d AS (SELECT pan AS p FROM cards) SELECT * FROM d";
        assert_eq!(
            sensitive_projection(sql, "postgres", &sensitive_patterns(), 1),
            vec![true]
        );
    }

    #[test]
    fn a_cte_alias_keeps_sensitive_lineage() {
        let positions = sensitive_projection(
            "WITH d AS (SELECT pan AS p FROM cards) SELECT p FROM d",
            "postgres",
            &sensitive_patterns(),
            1,
        );
        assert_eq!(positions, vec![true]);
    }

    #[test]
    fn a_union_masks_the_output_when_any_branch_is_sensitive() {
        let positions = sensitive_projection(
            "SELECT pan AS p FROM cards UNION ALL SELECT label AS p FROM archives",
            "postgres",
            &sensitive_patterns(),
            1,
        );
        assert_eq!(positions, vec![true]);
    }

    #[test]
    fn row_to_json_of_a_record_is_treated_as_sensitive() {
        let positions = sensitive_projection(
            "SELECT row_to_json(c) FROM cards c",
            "postgres",
            &sensitive_patterns(),
            1,
        );
        assert_eq!(positions, vec![true]);
    }

    #[test]
    fn a_returning_alias_keeps_sensitive_lineage() {
        let positions = sensitive_projection(
            "INSERT INTO cards (pan) VALUES ('4111111111111111') RETURNING pan AS p",
            "postgres",
            &sensitive_patterns(),
            1,
        );
        assert_eq!(positions, vec![true]);
    }

    #[test]
    fn unrecognized_query_shapes_redact_their_result_values() {
        let patterns = sensitive_patterns();
        for sql in [
            "WITH d AS (SELECT pan AS p FROM cards) SELECT p FROM d",
            "SELECT pan AS p FROM cards UNION ALL SELECT label AS p FROM archives",
            "SELECT row_to_json(c) FROM cards c",
            "INSERT INTO cards (pan) VALUES ('4111111111111111') RETURNING pan AS p",
        ] {
            let positions = sensitive_projection(sql, "postgres", &patterns, 1);
            let result = tablepro_core::QueryResult {
                columns: vec![tablepro_core::ColumnInfo {
                    name: "p".into(),
                    data_type: "text".into(),
                    nullable: true,
                    primary_key: false,
                    is_auto_increment: false,
                    default_value: None,
                    is_generated: false,
                    comment: None,
                    collation: None,
                    enum_type: None,
                    domain_type: None,
                }],
                rows: vec![vec![tablepro_core::Value::Text("sensitive".into())]],
                truncated: false,
            };
            let masked = crate::mask::apply_masking(result, &patterns, Some(&positions));
            assert_eq!(
                masked.rows[0][0],
                tablepro_core::Value::Text("***REDACTED***".into()),
                "{sql}"
            );
        }
    }
}
