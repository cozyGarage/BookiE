use mongodb::bson::{Bson, Document, doc};
use sqlparser::ast::{
    AssignmentTarget, BinaryOperator, Expr, FromTable, FunctionArg, FunctionArgExpr, FunctionArguments, ObjectNamePart,
    Statement, TableFactor, Value as SqlValue,
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
