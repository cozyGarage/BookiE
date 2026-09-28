use mongodb::bson::{Bson, Document, doc};
use sqlparser::ast::{
    AssignmentTarget, BinaryOperator, Expr, ObjectNamePart, Statement, TableFactor, Value as SqlValue,
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
    let TableFactor::Table {
        name,
        alias: None,
        args: None,
        ..
    } = &table.relation
    else {
        return Ok(None);
    };
    let identifiers = name.0.iter().map(ObjectNamePart::as_ident).collect::<Option<Vec<_>>>();
    let Some(identifiers) = identifiers else {
        return Ok(None);
    };
    let collection = match identifiers.as_slice() {
        [collection] => collection.value.clone(),
        [schema, collection] if schema.value == database => collection.value.clone(),
        _ => return Ok(None),
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

fn single_identifier(name: &sqlparser::ast::ObjectName) -> Option<String> {
    let [ObjectNamePart::Identifier(identifier)] = name.0.as_slice() else {
        return None;
    };
    Some(identifier.value.clone())
}

fn take_placeholder(expr: &Expr, params: &[Value], index: &mut usize) -> Result<Bson, DriverError> {
    let Expr::Value(value) = expr else {
        return Err(DriverError::Unsupported(
            "MongoDB grid updates require bound values".into(),
        ));
    };
    if !matches!(value.value, SqlValue::Placeholder(_)) {
        return Err(DriverError::Unsupported(
            "MongoDB grid updates require bound values".into(),
        ));
    }
    let Some(value) = params.get(*index) else {
        return Err(DriverError::Unsupported(
            "MongoDB grid update has too few bound values".into(),
        ));
    };
    *index += 1;
    value_to_bson(value)
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
                        "MongoDB grid update repeats a key condition".into(),
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
                    "MongoDB grid update has an unsupported key predicate".into(),
                ));
            };
            let value = take_placeholder(right, params, index)?;
            Ok(doc! { field: value })
        }
        Expr::IsNull(inner) => {
            let Some(field) = identifier_from_expr(inner) else {
                return Err(DriverError::Unsupported(
                    "MongoDB grid update has an unsupported NULL key predicate".into(),
                ));
            };
            Ok(doc! { field: Bson::Null })
        }
        _ => Err(DriverError::Unsupported(
            "MongoDB grid update has an unsupported key predicate".into(),
        )),
    }
}

fn identifier_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(identifier) => Some(identifier.value.clone()),
        Expr::CompoundIdentifier(identifiers) if identifiers.len() == 1 => Some(identifiers[0].value.clone()),
        _ => None,
    }
}
