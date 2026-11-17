use bookie_core::{DriverError, ViewEditability};
use sqlparser::ast::{Expr, GroupByExpr, Select, SelectItem, SetExpr, Statement, TableFactor};
use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;
use sqlx::{MySqlConnection, Row};

use crate::map_sqlx_error;

pub(super) async fn fetch(
    connection: &mut MySqlConnection,
    schema: Option<&str>,
    view: &str,
) -> Result<ViewEditability, DriverError> {
    let Some(info) = view_info(connection, schema, view).await? else {
        return Ok(ViewEditability::NotApplicable);
    };
    if !info.updatable {
        return Ok(ViewEditability::NotUpdatable);
    }
    let Some(description) = info
        .definition
        .as_deref()
        .and_then(|definition| parse_view_definition(&info.schema, definition))
    else {
        return Ok(ViewEditability::NotUpdatable);
    };
    let primary_key = primary_key(connection, &description.schema, &description.table).await?;
    if primary_key.is_empty() {
        return Ok(ViewEditability::KeyNotExposed);
    }
    let Some(key_columns) = exposed_keys(&description.origins, &primary_key) else {
        return Ok(ViewEditability::KeyNotExposed);
    };

    Ok(ViewEditability::Editable {
        key_columns,
        read_only_columns: description.read_only_columns,
        insertable: false,
    })
}

async fn view_info(
    connection: &mut MySqlConnection,
    schema: Option<&str>,
    view: &str,
) -> Result<Option<ViewInfo>, DriverError> {
    Ok(sqlx::query(
        "SELECT CAST(table_schema AS CHAR), is_updatable, view_definition
         FROM information_schema.views
         WHERE table_schema = COALESCE(?, DATABASE()) AND table_name = ?",
    )
    .bind(schema)
    .bind(view)
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_sqlx_error)?
    .map(|row| ViewInfo {
        schema: row.get(0),
        updatable: row.get::<String, _>(1) == "YES",
        definition: row.get(2),
    }))
}

async fn primary_key(connection: &mut MySqlConnection, schema: &str, table: &str) -> Result<Vec<String>, DriverError> {
    Ok(sqlx::query(
        "SELECT CAST(column_name AS CHAR)
         FROM information_schema.key_column_usage
         WHERE table_schema = ? AND table_name = ? AND constraint_name = 'PRIMARY'
         ORDER BY ordinal_position",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?
    .into_iter()
    .map(|row| row.get::<String, _>(0))
    .collect())
}

fn exposed_keys(origins: &[Origin], primary_key: &[String]) -> Option<Vec<String>> {
    primary_key
        .iter()
        .map(|key| {
            origins
                .iter()
                .find(|origin| origin.column.eq_ignore_ascii_case(key))
                .map(|origin| origin.name.clone())
        })
        .collect()
}

struct ViewInfo {
    schema: String,
    updatable: bool,
    definition: Option<String>,
}

struct ViewColumns {
    schema: String,
    table: String,
    origins: Vec<Origin>,
    read_only_columns: Vec<String>,
}

struct Origin {
    name: String,
    column: String,
}

fn parse_view_definition(default_schema: &str, definition: &str) -> Option<ViewColumns> {
    let statements = Parser::parse_sql(&MySqlDialect {}, definition).ok()?;
    let [Statement::Query(query)] = statements.as_slice() else {
        return None;
    };
    if query.with.is_some() || query.limit_clause.is_some() || query.fetch.is_some() || !query.pipe_operators.is_empty()
    {
        return None;
    }
    let SetExpr::Select(select) = query.body.as_ref() else {
        return None;
    };
    if select.distinct.is_some()
        || select.having.is_some()
        || !matches!(&select.group_by, GroupByExpr::Expressions(items, modifiers) if items.is_empty() && modifiers.is_empty())
    {
        return None;
    }
    let [source] = select.from.as_slice() else { return None };
    if !source.joins.is_empty() {
        return None;
    }
    let TableFactor::Table {
        name,
        alias,
        args: None,
        version: None,
        sample: None,
        ..
    } = &source.relation
    else {
        return None;
    };
    if alias.as_ref().is_some_and(|alias| !alias.columns.is_empty()) {
        return None;
    }
    let (schema, table) = match name.0.as_slice() {
        [table] => (default_schema, table.as_ident()?.value.as_str()),
        [schema, table] => (schema.as_ident()?.value.as_str(), table.as_ident()?.value.as_str()),
        _ => return None,
    };
    let qualifier = alias.as_ref().map(|alias| alias.name.value.as_str());
    projected_columns(select, schema, table, qualifier)
}

fn projected_columns(select: &Select, schema: &str, table: &str, qualifier: Option<&str>) -> Option<ViewColumns> {
    let mut description = ViewColumns {
        schema: schema.to_string(),
        table: table.to_string(),
        origins: Vec::new(),
        read_only_columns: Vec::new(),
    };
    for item in &select.projection {
        let (expression, name) = match item {
            SelectItem::UnnamedExpr(expression) => (
                expression,
                direct_column(expression, schema, table, qualifier)?.to_string(),
            ),
            SelectItem::ExprWithAlias { expr, alias } => (expr, alias.value.clone()),
            _ => return None,
        };
        match direct_column(expression, schema, table, qualifier) {
            Some(column) => description.origins.push(Origin {
                name,
                column: column.to_string(),
            }),
            None => description.read_only_columns.push(name),
        }
    }
    Some(description)
}

fn direct_column<'a>(expression: &'a Expr, schema: &str, table: &str, alias: Option<&str>) -> Option<&'a str> {
    match expression {
        Expr::Identifier(column) => Some(&column.value),
        Expr::CompoundIdentifier(parts) => match parts.as_slice() {
            [qualifier, column] if qualifier.value.eq_ignore_ascii_case(alias.unwrap_or(table)) => Some(&column.value),
            [source_schema, source_table, column]
                if alias.is_none() && source_schema.value == schema && source_table.value == table =>
            {
                Some(&column.value)
            }
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_aliases_expose_keys_and_computed_keys_do_not() {
        let direct = parse_view_definition(
            "shop",
            "SELECT b.id AS item_id, b.part, UPPER(b.name) AS upper_name FROM shop.base AS b",
        )
        .unwrap();
        assert_eq!(
            exposed_keys(&direct.origins, &["id".into(), "part".into()]),
            Some(vec!["item_id".into(), "part".into()])
        );
        assert_eq!(direct.read_only_columns, ["upper_name"]);
        let spoof = parse_view_definition("shop", "SELECT id + 0 AS id, part FROM shop.base").unwrap();
        assert_eq!(exposed_keys(&spoof.origins, &["id".into(), "part".into()]), None);
    }

    #[test]
    fn quoted_dots_are_identifier_content() {
        let description = parse_view_definition(
            "default",
            "SELECT `b`.`i.d` AS `key.alias` FROM `sch.ema`.`tab.le` AS `b`",
        )
        .unwrap();
        assert_eq!(
            (description.schema.as_str(), description.table.as_str()),
            ("sch.ema", "tab.le")
        );
        assert_eq!(description.origins[0].column, "i.d");
        assert_eq!(description.origins[0].name, "key.alias");
    }

    #[test]
    fn complex_or_unknown_view_definitions_are_refused() {
        for definition in [
            "WITH t AS (SELECT id FROM base) SELECT id FROM t",
            "SELECT a.id FROM a JOIN b ON a.id = b.id",
            "SELECT id FROM a UNION SELECT id FROM b",
            "SELECT id FROM (SELECT id FROM base) AS b",
            "SELECT * FROM base",
            "SELECT id FROM base; SELECT id FROM other",
            "SELECT DISTINCT id FROM base",
            "SELECT id FROM base GROUP BY id",
            "SELECT other.id FROM base",
            "invalid SQL",
        ] {
            assert!(parse_view_definition("shop", definition).is_none(), "{definition}");
        }
    }
}
