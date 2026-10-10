use mongodb::bson::{Bson, Document, doc};
use sqlparser::ast::{
    AssignmentTarget, BinaryOperator, Expr, FromTable, FunctionArg, FunctionArgExpr, FunctionArguments, GroupByExpr,
    LimitClause, ObjectNamePart, Query, Select, SelectItem, SetExpr, Statement, TableFactor, Value as SqlValue,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use super::codec::value_to_bson;
use tablepro_core::{DriverError, Value};

pub(super) struct KeyedUpdate {
    pub(super) collection: String,
    pub(super) filter: Document,
    pub(super) set: Document,
}

pub(super) struct KeyedDelete {
    pub(super) collection: String,
    pub(super) filter: Document,
}

pub(super) struct KeyedValueSelect {
    pub(super) collection: String,
    pub(super) column: String,
    pub(super) key: Bson,
}

pub(super) struct KeysetPageSelect {
    pub(super) collection: String,
    pub(super) key: Bson,
    pub(super) limit: i64,
}

pub(super) fn parse_keyset_page_select(
    sql: &str,
    params: &[Value],
    database: &str,
) -> Result<Option<KeysetPageSelect>, DriverError> {
    let statements = Parser::parse_sql(&GenericDialect {}, sql)
        .map_err(|error| DriverError::Unsupported(format!("invalid parameterized MongoDB page query: {error}")))?;
    let [Statement::Query(query)] = statements.as_slice() else {
        return Ok(None);
    };
    let Some((select, limit)) = keyset_page_query(query) else {
        return Ok(None);
    };
    if !matches!(select.projection.as_slice(), [SelectItem::Wildcard(_)]) {
        return Ok(None);
    }
    let relation = &select.from[0];
    if !relation.joins.is_empty() {
        return Ok(None);
    }
    let Some(collection) = collection_named(&relation.relation, database) else {
        return Ok(None);
    };
    let Some(Expr::BinaryOp {
        left,
        op: BinaryOperator::Gt,
        right,
    }) = select.selection.as_ref()
    else {
        return Ok(None);
    };
    if identifier_from_expr(left).as_deref() != Some("_id") || params.len() != 1 {
        return Ok(None);
    }
    let mut index = 0;
    let key = take_placeholder(right, params, &mut index)?;
    if index != params.len() {
        return Ok(None);
    }
    Ok(Some(KeysetPageSelect { collection, key, limit }))
}

fn keyset_page_query(query: &Query) -> Option<(&Select, i64)> {
    let mut unpaged_query = query.clone();
    unpaged_query.limit_clause = None;
    simple_value_select(&unpaged_query)?;
    let Some(LimitClause::LimitOffset {
        limit: Some(Expr::Value(limit)),
        offset: Some(offset),
        limit_by,
    }) = &query.limit_clause
    else {
        return None;
    };
    let SqlValue::Number(limit, _) = &limit.value else {
        return None;
    };
    let limit = limit.parse::<i64>().ok()?;
    if limit <= 0 || !limit_by.is_empty() || offset.value.to_string() != "0" {
        return None;
    }
    let SetExpr::Select(select) = query.body.as_ref() else {
        return None;
    };
    Some((select, limit))
}

pub(super) fn parse_keyed_value_select(
    sql: &str,
    params: &[Value],
    database: &str,
) -> Result<Option<KeyedValueSelect>, DriverError> {
    let statements = Parser::parse_sql(&GenericDialect {}, sql)
        .map_err(|error| DriverError::Unsupported(format!("invalid parameterized MongoDB value query: {error}")))?;
    let [Statement::Query(query)] = statements.as_slice() else {
        return Ok(None);
    };
    let Some(select) = simple_value_select(query) else {
        return Ok(None);
    };
    let [SelectItem::UnnamedExpr(Expr::Identifier(column))] = select.projection.as_slice() else {
        return Ok(None);
    };
    if column.value.starts_with('$') || column.value.contains('.') {
        return Ok(None);
    }
    let relation = &select.from[0];
    if !relation.joins.is_empty() {
        return Ok(None);
    }
    let Some(collection) = collection_named(&relation.relation, database) else {
        return Ok(None);
    };
    let Some(Expr::BinaryOp {
        left,
        op: BinaryOperator::Eq,
        right,
    }) = select.selection.as_ref()
    else {
        return Ok(None);
    };
    if identifier_from_expr(left).as_deref() != Some("_id") || params.len() != 1 {
        return Ok(None);
    }
    let mut index = 0;
    let key = take_placeholder(right, params, &mut index)?;
    if index != params.len() {
        return Ok(None);
    }
    Ok(Some(KeyedValueSelect {
        collection,
        column: column.value.clone(),
        key,
    }))
}

fn simple_value_select(query: &Query) -> Option<&Select> {
    if query.with.is_some()
        || query.order_by.is_some()
        || query.limit_clause.is_some()
        || query.fetch.is_some()
        || !query.locks.is_empty()
        || query.for_clause.is_some()
        || query.settings.is_some()
        || query.format_clause.is_some()
        || !query.pipe_operators.is_empty()
    {
        return None;
    }
    let SetExpr::Select(select) = query.body.as_ref() else {
        return None;
    };
    if select.distinct.is_some()
        || select.top.is_some()
        || select.exclude.is_some()
        || select.into.is_some()
        || select.from.len() != 1
        || !select.lateral_views.is_empty()
        || select.prewhere.is_some()
        || !matches!(&select.group_by, GroupByExpr::Expressions(expressions, _) if expressions.is_empty())
        || !select.cluster_by.is_empty()
        || !select.distribute_by.is_empty()
        || !select.sort_by.is_empty()
        || select.having.is_some()
        || !select.named_window.is_empty()
        || select.qualify.is_some()
        || select.value_table_mode.is_some()
        || select.connect_by.is_some()
    {
        return None;
    }
    Some(select)
}

pub(super) fn parse_keyed_update(
    sql: &str,
    params: &[Value],
    database: &str,
) -> Result<Option<KeyedUpdate>, DriverError> {
    let statements = Parser::parse_sql(&GenericDialect {}, sql)
        .map_err(|error| DriverError::Unsupported(format!("invalid parameterized MongoDB update: {error}")))?;
    let [
        Statement::Update {
            table,
            assignments,
            from,
            selection: Some(selection),
            returning: None,
            or: None,
        },
    ] = statements.as_slice()
    else {
        return Ok(None);
    };
    if from.is_some() || !table.joins.is_empty() {
        return Ok(None);
    }
    let Some(collection) = collection_named(&table.relation, database) else {
        return Ok(None);
    };
    if assignments.is_empty() {
        return Ok(None);
    }

    let mut parameter_index = 0;
    let mut set = Document::new();
    for assignment in assignments {
        let AssignmentTarget::ColumnName(column) = &assignment.target else {
            return Ok(None);
        };
        let Some(column) = single_identifier(column) else {
            return Ok(None);
        };
        let value = take_placeholder(&assignment.value, params, &mut parameter_index)?;
        if set.insert(column, value).is_some() {
            return Ok(None);
        }
    }
    let filter = selector_from_expr(selection, params, &mut parameter_index)?;
    if parameter_index != params.len() || filter.is_empty() {
        return Ok(None);
    }
    Ok(Some(KeyedUpdate {
        collection,
        filter,
        set,
    }))
}

pub(super) fn parse_keyed_delete(
    sql: &str,
    params: &[Value],
    database: &str,
) -> Result<Option<KeyedDelete>, DriverError> {
    let statements = Parser::parse_sql(&GenericDialect {}, sql)
        .map_err(|error| DriverError::Unsupported(format!("invalid parameterized MongoDB delete: {error}")))?;
    let [Statement::Delete(delete)] = statements.as_slice() else {
        return Ok(None);
    };
    if !delete.tables.is_empty()
        || delete.using.is_some()
        || delete.returning.is_some()
        || !delete.order_by.is_empty()
        || delete.limit.is_some()
    {
        return Ok(None);
    }
    let Some(selection) = &delete.selection else {
        return Ok(None);
    };
    let tables = match &delete.from {
        FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => tables,
    };
    let [table] = tables.as_slice() else {
        return Ok(None);
    };
    if !table.joins.is_empty() {
        return Ok(None);
    }
    let Some(collection) = collection_named(&table.relation, database) else {
        return Ok(None);
    };
    let mut parameter_index = 0;
    let filter = selector_from_expr(selection, params, &mut parameter_index)?;
    if parameter_index != params.len() || filter.is_empty() {
        return Ok(None);
    }
    Ok(Some(KeyedDelete { collection, filter }))
}

fn collection_named(relation: &TableFactor, database: &str) -> Option<String> {
    let TableFactor::Table {
        name,
        alias: None,
        args: None,
        ..
    } = relation
    else {
        return None;
    };
    let identifiers = name
        .0
        .iter()
        .map(ObjectNamePart::as_ident)
        .collect::<Option<Vec<_>>>()?;
    match identifiers.as_slice() {
        [collection] => Some(collection.value.clone()),
        [schema, collection] if schema.value == database => Some(collection.value.clone()),
        _ => None,
    }
}

fn single_identifier(name: &sqlparser::ast::ObjectName) -> Option<String> {
    let [ObjectNamePart::Identifier(identifier)] = name.0.as_slice() else {
        return None;
    };
    Some(identifier.value.clone())
}

fn take_placeholder(expr: &Expr, params: &[Value], index: &mut usize) -> Result<Bson, DriverError> {
    value_to_bson(&take_placeholder_value(expr, params, index)?)
}

fn take_placeholder_value(expr: &Expr, params: &[Value], index: &mut usize) -> Result<Value, DriverError> {
    let Expr::Value(value) = expr else {
        return Err(DriverError::Unsupported(
            "MongoDB grid writes require bound values".into(),
        ));
    };
    if !matches!(value.value, SqlValue::Placeholder(_)) {
        return Err(DriverError::Unsupported(
            "MongoDB grid writes require bound values".into(),
        ));
    }
    let Some(value) = params.get(*index) else {
        return Err(DriverError::Unsupported(
            "MongoDB grid write has too few bound values".into(),
        ));
    };
    *index += 1;
    Ok(value.clone())
}

fn selector_from_expr(expr: &Expr, params: &[Value], index: &mut usize) -> Result<Document, DriverError> {
    match expr {
        Expr::Nested(inner) => selector_from_expr(inner, params, index),
        Expr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => {
            let mut selector = selector_from_expr(left, params, index)?;
            let right = selector_from_expr(right, params, index)?;
            for (key, value) in right {
                if selector.insert(key, value).is_some() {
                    return Err(DriverError::Unsupported(
                        "MongoDB grid write repeats a key condition".into(),
                    ));
                }
            }
            Ok(selector)
        }
        Expr::BinaryOp {
            left,
            op: BinaryOperator::Eq,
            right,
        } => {
            let Some(field) = identifier_from_expr(left) else {
                return Err(DriverError::Unsupported(
                    "MongoDB grid write has an unsupported key predicate".into(),
                ));
            };
            let raw_value = take_placeholder_value(right, params, index)?;
            if matches!(&raw_value, Value::Undecodable(kind) if kind == "missing BSON field") {
                return Ok(doc! { field: { "$exists": false } });
            }
            let value = value_to_bson(&raw_value)?;
            if matches!(value, Bson::Null) {
                Ok(explicit_null_selector(field))
            } else {
                Ok(doc! { field: value })
            }
        }
        Expr::IsNull(inner) => {
            let Some(field) = identifier_from_expr(inner) else {
                return Err(DriverError::Unsupported(
                    "MongoDB grid write has an unsupported NULL key predicate".into(),
                ));
            };
            Ok(explicit_null_selector(field))
        }
        Expr::Function(function)
            if single_identifier(&function.name).as_deref() == Some("tablepro_mongodb_exact_field_set") =>
        {
            exact_field_set_selector(function, params, index)
        }
        _ => Err(DriverError::Unsupported(
            "MongoDB grid write has an unsupported key predicate".into(),
        )),
    }
}

fn exact_field_set_selector(
    function: &sqlparser::ast::Function,
    params: &[Value],
    index: &mut usize,
) -> Result<Document, DriverError> {
    let invalid = || DriverError::Unsupported("MongoDB delete has an invalid document field-set guard".into());
    if function.parameters != FunctionArguments::None
        || function.uses_odbc_syntax
        || function.filter.is_some()
        || function.null_treatment.is_some()
        || function.over.is_some()
        || !function.within_group.is_empty()
    {
        return Err(invalid());
    }
    let FunctionArguments::List(arguments) = &function.args else {
        return Err(invalid());
    };
    if arguments.duplicate_treatment.is_some() || !arguments.clauses.is_empty() {
        return Err(invalid());
    }
    let [FunctionArg::Unnamed(FunctionArgExpr::Expr(value))] = arguments.args.as_slice() else {
        return Err(invalid());
    };
    let value = take_placeholder_value(value, params, index)?;
    let Bson::Array(expected_fields) = value_to_bson(&value)? else {
        return Err(invalid());
    };
    if expected_fields.iter().any(|field| !matches!(field, Bson::String(_))) {
        return Err(invalid());
    }
    Ok(doc! {
        "$expr": {
            "$setEquals": [
                { "$map": { "input": { "$objectToArray": "$$ROOT" }, "as": "field", "in": "$$field.k" } },
                expected_fields,
            ]
        }
    })
}

fn explicit_null_selector(field: String) -> Document {
    doc! { field: { "$eq": Bson::Null, "$exists": true, "$not": { "$type": "array" } } }
}

fn identifier_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(identifier) => Some(identifier.value.clone()),
        Expr::CompoundIdentifier(identifiers) if identifiers.len() == 1 => Some(identifiers[0].value.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod value_select_tests {
    use super::*;

    #[test]
    fn value_select_binds_only_the_selected_field_and_native_id() {
        let id = "0123456789abcdef01234567";
        let parsed = parse_keyed_value_select(
            "SELECT \"payload\" FROM \"appdb\".\"records\" WHERE \"_id\" = ?",
            &[Value::Json(serde_json::json!({"$oid": id}))],
            "appdb",
        )
        .unwrap()
        .unwrap();

        assert_eq!(parsed.collection, "records");
        assert_eq!(parsed.column, "payload");
        assert_eq!(parsed.key, Bson::ObjectId(id.parse().unwrap()));
    }

    #[test]
    fn value_select_refuses_extra_predicates_and_query_shapes() {
        for sql in [
            "SELECT payload FROM records WHERE _id = ? AND tenant = ?",
            "SELECT payload FROM records WHERE _id = ? ORDER BY _id",
            "SELECT payload FROM records WHERE _id = ? UNION SELECT payload FROM other",
            "SELECT payload FROM records",
            "SELECT \"nested.value\" FROM records WHERE _id = ?",
            "SELECT \"$value\" FROM records WHERE _id = ?",
            "DELETE FROM records WHERE _id = ?",
        ] {
            assert!(
                parse_keyed_value_select(sql, &[Value::Text("id".into())], "appdb")
                    .unwrap()
                    .is_none(),
                "accepted {sql}"
            );
        }
    }
}

#[cfg(test)]
mod keyset_page_tests {
    use super::*;

    #[test]
    fn keyset_page_binds_one_id_cursor_and_window() {
        let parsed = parse_keyset_page_select(
            "SELECT * FROM \"appdb\".\"records\" WHERE \"_id\" > ? LIMIT 50 OFFSET 0",
            &[Value::Text("last".into())],
            "appdb",
        )
        .unwrap()
        .unwrap();

        assert_eq!(parsed.collection, "records");
        assert_eq!(parsed.key, Bson::String("last".into()));
        assert_eq!(parsed.limit, 50);
    }

    #[test]
    fn keyset_page_refuses_other_predicates_projection_and_windows() {
        for sql in [
            "SELECT * FROM records WHERE _id >= ? LIMIT 50 OFFSET 0",
            "SELECT * FROM records WHERE _id > ? OR _id = ? LIMIT 50 OFFSET 0",
            "SELECT * FROM records JOIN archive ON true WHERE _id > ? LIMIT 50 OFFSET 0",
            "SELECT * FROM records WHERE value > ? LIMIT 50 OFFSET 0",
            "SELECT value FROM records WHERE _id > ? LIMIT 50 OFFSET 0",
            "SELECT * FROM records WHERE _id > ? LIMIT 0 OFFSET 0",
            "SELECT * FROM records WHERE _id > ? LIMIT 50 OFFSET 1",
            "SELECT * FROM records WHERE _id > ? ORDER BY value LIMIT 50 OFFSET 0",
        ] {
            assert!(
                parse_keyset_page_select(sql, &[Value::Int(1)], "appdb")
                    .unwrap()
                    .is_none()
            );
        }
    }
}
