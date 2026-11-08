use bookie_core::sql_syntax::SqlGrammar;
use bookie_core::sql_syntax::script::{LexicalSettings, ScriptPlan, ScriptTokenKind};
use bookie_core::{ColumnInfo, TableInfo};
use sqlparser::ast::{
    Expr, GroupByExpr, Ident, ObjectName, ObjectNamePart, OrderByKind, Query, Select, SelectFlavor, SelectItem,
    SelectItemQualifiedWildcardKind, SetExpr, Statement, TableFactor, WildcardAdditionalOptions,
};
use sqlparser::dialect::{Dialect, GenericDialect, MsSqlDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;

const RECURSION_LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTable {
    pub table: TableInfo,
    pub items: Vec<SourceItem>,
    pub(super) grammar: SqlGrammar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceItem {
    Column(SourceName),
    Wildcard,
    Expression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceName {
    pub name: String,
    pub quoted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceRefusal {
    NotOneSelect,
    MoreThanOneTable,
    Collapsed,
}

pub fn source_table(sql: &str, grammar: SqlGrammar) -> Result<SourceTable, SourceRefusal> {
    let query = parse_query(sql, grammar)?;
    single_table_query(query.as_ref(), grammar)
}

fn parse_query(sql: &str, grammar: SqlGrammar) -> Result<Box<Query>, SourceRefusal> {
    let dialect = parser_dialect(grammar).ok_or(SourceRefusal::NotOneSelect)?;
    let plan = ScriptPlan::build(sql, grammar, LexicalSettings::default_for(grammar));
    if !plan.diagnostics().is_empty()
        || plan
            .tokens(0..sql.len())
            .any(|token| matches!(token.kind, ScriptTokenKind::Directive | ScriptTokenKind::Unterminated))
    {
        return Err(SourceRefusal::NotOneSelect);
    }
    let mut statements = Parser::new(dialect.as_ref())
        .with_recursion_limit(RECURSION_LIMIT)
        .try_with_sql(sql)
        .and_then(|mut parser| parser.parse_statements())
        .map_err(|_| SourceRefusal::NotOneSelect)?
        .into_iter();
    let Some(Statement::Query(query)) = statements.next() else {
        return Err(SourceRefusal::NotOneSelect);
    };
    match statements.next() {
        None => Ok(query),
        Some(_) => Err(SourceRefusal::NotOneSelect),
    }
}

fn parser_dialect(grammar: SqlGrammar) -> Option<Box<dyn Dialect>> {
    Some(match grammar {
        SqlGrammar::PostgreSql => Box::new(PostgreSqlDialect {}),
        SqlGrammar::MySql => Box::new(MySqlDialect {}),
        SqlGrammar::Sqlite => Box::new(SQLiteDialect {}),
        SqlGrammar::MsSql => Box::new(MsSqlDialect {}),
        SqlGrammar::ClickHouse => Box::new(GenericDialect {}),
    })
}

fn plain_select(query: &Query) -> Result<&Select, SourceRefusal> {
    if query.with.is_some() {
        return Err(SourceRefusal::MoreThanOneTable);
    }
    if query.for_clause.is_some() || query.format_clause.is_some() || !query.pipe_operators.is_empty() {
        return Err(SourceRefusal::NotOneSelect);
    }
    if let Some(order) = &query.order_by {
        let OrderByKind::Expressions(expressions) = &order.kind else {
            return Err(SourceRefusal::Collapsed);
        };
        if order.interpolate.is_some()
            || expressions
                .iter()
                .any(|item| item.with_fill.is_some() || !row_expression(&item.expr))
        {
            return Err(SourceRefusal::Collapsed);
        }
    }
    let select = match query.body.as_ref() {
        SetExpr::Select(select) => select,
        SetExpr::SetOperation { .. } | SetExpr::Query(_) => return Err(SourceRefusal::MoreThanOneTable),
        _ => return Err(SourceRefusal::NotOneSelect),
    };
    let plain_grouping = matches!(&select.group_by, GroupByExpr::Expressions(expressions, modifiers)
        if expressions.is_empty() && modifiers.is_empty());
    if select.distinct.is_some() || !plain_grouping || select.having.is_some() || select.qualify.is_some() {
        return Err(SourceRefusal::Collapsed);
    }
    if select.into.is_some()
        || select.exclude.is_some()
        || !select.lateral_views.is_empty()
        || !select.connect_by.is_empty()
        || select.value_table_mode.is_some()
        || select.flavor != SelectFlavor::Standard
    {
        return Err(SourceRefusal::NotOneSelect);
    }
    Ok(select)
}

fn single_table_query(query: &Query, grammar: SqlGrammar) -> Result<SourceTable, SourceRefusal> {
    let select = plain_select(query)?;
    let [from] = select.from.as_slice() else {
        return Err(match select.from.is_empty() {
            true => SourceRefusal::NotOneSelect,
            false => SourceRefusal::MoreThanOneTable,
        });
    };
    if !from.joins.is_empty() {
        return Err(SourceRefusal::MoreThanOneTable);
    }
    let TableFactor::Table {
        name,
        alias,
        args: None,
        version: None,
        with_ordinality: false,
        partitions,
        json_path: None,
        sample: None,
        ..
    } = &from.relation
    else {
        return Err(SourceRefusal::MoreThanOneTable);
    };
    if !partitions.is_empty() || alias.as_ref().is_some_and(|alias| !alias.columns.is_empty()) {
        return Err(SourceRefusal::MoreThanOneTable);
    }
    let parts = identifiers(name).ok_or(SourceRefusal::MoreThanOneTable)?;
    let (schema, table) = match parts.as_slice() {
        [table] => (None, (*table).clone()),
        [schema, table] => (Some((*schema).clone()), (*table).clone()),
        _ => return Err(SourceRefusal::MoreThanOneTable),
    };
    let qualifiers: Vec<&Ident> = alias.as_ref().map_or_else(|| vec![&table], |alias| vec![&alias.name]);
    let items = select
        .projection
        .iter()
        .map(|item| source_item(item, &qualifiers, schema.as_ref(), grammar))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SourceTable {
        table: TableInfo {
            schema: schema.map(|ident| resolved_name(&ident, grammar)),
            name: resolved_name(&table, grammar),
        },
        items,
        grammar,
    })
}

fn identifiers(name: &ObjectName) -> Option<Vec<&Ident>> {
    name.0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident) => Some(ident),
            ObjectNamePart::Function(_) => None,
        })
        .collect()
}

fn source_item(
    item: &SelectItem,
    qualifiers: &[&Ident],
    schema: Option<&Ident>,
    grammar: SqlGrammar,
) -> Result<SourceItem, SourceRefusal> {
    match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => match row_expression(expr) {
            true => Ok(column(expr, qualifiers, schema, grammar)),
            false => Err(SourceRefusal::Collapsed),
        },
        SelectItem::ExprWithAliases { .. } => Err(SourceRefusal::Collapsed),
        SelectItem::Wildcard(options) => match plain_wildcard(options) {
            true => Ok(SourceItem::Wildcard),
            false => Err(SourceRefusal::NotOneSelect),
        },
        SelectItem::QualifiedWildcard(SelectItemQualifiedWildcardKind::ObjectName(name), options) => {
            let parts = identifiers(name).ok_or(SourceRefusal::MoreThanOneTable)?;
            match (
                plain_wildcard(options),
                names_the_table(&parts, qualifiers, schema, grammar),
            ) {
                (true, true) => Ok(SourceItem::Wildcard),
                (false, _) => Err(SourceRefusal::NotOneSelect),
                (true, false) => Err(SourceRefusal::MoreThanOneTable),
            }
        }
        SelectItem::QualifiedWildcard(SelectItemQualifiedWildcardKind::Expr(_), _) => {
            Err(SourceRefusal::MoreThanOneTable)
        }
    }
}

fn column(expr: &Expr, qualifiers: &[&Ident], schema: Option<&Ident>, grammar: SqlGrammar) -> SourceItem {
    let (qualifier, name): (&[Ident], &Ident) = match expr {
        Expr::Identifier(ident) => (&[], ident),
        Expr::CompoundIdentifier(parts) => match parts.split_last() {
            Some((name, qualifier)) => (qualifier, name),
            None => return SourceItem::Expression,
        },
        Expr::Nested(inner) => return column(inner, qualifiers, schema, grammar),
        _ => return SourceItem::Expression,
    };
    let qualifier: Vec<&Ident> = qualifier.iter().collect();
    if !qualifier.is_empty() && !names_the_table(&qualifier, qualifiers, schema, grammar) {
        return SourceItem::Expression;
    }
    if qualifier.is_empty() && name.quote_style.is_none() && value_keyword(&name.value, grammar) {
        return SourceItem::Expression;
    }
    SourceItem::Column(SourceName {
        name: resolved_name(name, grammar),
        quoted: name.quote_style.is_some(),
    })
}

fn value_keyword(name: &str, grammar: SqlGrammar) -> bool {
    const POSTGRES: [&str; 12] = [
        "current_catalog",
        "current_date",
        "current_role",
        "current_schema",
        "current_time",
        "current_timestamp",
        "current_user",
        "localtime",
        "localtimestamp",
        "session_user",
        "system_user",
        "user",
    ];
    grammar == SqlGrammar::PostgreSql && POSTGRES.iter().any(|keyword| keyword.eq_ignore_ascii_case(name))
}

fn names_the_table(parts: &[&Ident], qualifiers: &[&Ident], schema: Option<&Ident>, grammar: SqlGrammar) -> bool {
    match parts {
        [name] => qualifiers.iter().any(|qualifier| same_ident(qualifier, name, grammar)),
        [part_schema, name] => {
            schema.is_some_and(|schema| same_ident(schema, part_schema, grammar))
                && qualifiers.iter().any(|qualifier| same_ident(qualifier, name, grammar))
        }
        _ => false,
    }
}

fn resolved_name(ident: &Ident, grammar: SqlGrammar) -> String {
    if ident.quote_style.is_none() && matches!(grammar, SqlGrammar::PostgreSql) {
        ident.value.to_ascii_lowercase()
    } else {
        ident.value.clone()
    }
}

pub(super) fn same_name(left: &str, right: &str, grammar: SqlGrammar) -> bool {
    match grammar {
        SqlGrammar::MySql | SqlGrammar::Sqlite | SqlGrammar::MsSql => left.eq_ignore_ascii_case(right),
        SqlGrammar::PostgreSql | SqlGrammar::ClickHouse => left == right,
    }
}

fn same_ident(left: &Ident, right: &Ident, grammar: SqlGrammar) -> bool {
    let (left, right) = (resolved_name(left, grammar), resolved_name(right, grammar));
    match grammar {
        SqlGrammar::Sqlite => left.eq_ignore_ascii_case(&right),
        _ => left == right,
    }
}

fn row_expression(expr: &Expr) -> bool {
    match expr {
        Expr::Identifier(_) | Expr::CompoundIdentifier(_) | Expr::Value(_) | Expr::TypedString(_) => true,
        Expr::Nested(expr)
        | Expr::UnaryOp { expr, .. }
        | Expr::IsFalse(expr)
        | Expr::IsNotFalse(expr)
        | Expr::IsTrue(expr)
        | Expr::IsNotTrue(expr)
        | Expr::IsNull(expr)
        | Expr::IsNotNull(expr)
        | Expr::IsUnknown(expr)
        | Expr::IsNotUnknown(expr) => row_expression(expr),
        Expr::BinaryOp { left, right, .. }
        | Expr::IsDistinctFrom(left, right)
        | Expr::IsNotDistinctFrom(left, right) => row_expression(left) && row_expression(right),
        Expr::Between { expr, low, high, .. } => row_expression(expr) && row_expression(low) && row_expression(high),
        Expr::InList { expr, list, .. } => row_expression(expr) && list.iter().all(row_expression),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand.as_deref().is_none_or(row_expression)
                && conditions
                    .iter()
                    .all(|branch| row_expression(&branch.condition) && row_expression(&branch.result))
                && else_result.as_deref().is_none_or(row_expression)
        }
        _ => false,
    }
}

fn plain_wildcard(options: &WildcardAdditionalOptions) -> bool {
    options.opt_ilike.is_none()
        && options.opt_exclude.is_none()
        && options.opt_except.is_none()
        && options.opt_replace.is_none()
        && options.opt_rename.is_none()
        && options.opt_alias.is_none()
}

pub(super) fn source_columns(source: &SourceTable, columns: &[ColumnInfo], width: usize) -> Option<Vec<Option<usize>>> {
    let mut mapped = Vec::with_capacity(width);
    for item in &source.items {
        match item {
            SourceItem::Wildcard => mapped.extend((0..columns.len()).map(Some)),
            SourceItem::Expression => mapped.push(None),
            SourceItem::Column(name) => mapped.push(column_position(columns, name, source.grammar)),
        }
    }
    (mapped.len() == width).then_some(mapped)
}

pub(super) fn source_columns_for_result(
    source: &SourceTable,
    columns: &[ColumnInfo],
    result_names: &[String],
) -> Option<Vec<Option<usize>>> {
    let mapped = source_columns(source, columns, result_names.len())?;
    let mut offset = 0usize;
    for item in &source.items {
        if matches!(item, SourceItem::Wildcard) {
            let end = offset.checked_add(columns.len())?;
            let names = result_names.get(offset..end)?;
            if !names
                .iter()
                .zip(columns)
                .all(|(name, column)| same_name(name, &column.name, source.grammar))
            {
                return None;
            }
            offset = end;
        } else {
            offset = offset.checked_add(1)?;
        }
    }
    Some(mapped)
}

fn column_position(columns: &[ColumnInfo], name: &SourceName, grammar: SqlGrammar) -> Option<usize> {
    let insensitive = matches!(grammar, SqlGrammar::MySql | SqlGrammar::Sqlite);
    let mut matches = columns.iter().enumerate().filter(|(_, column)| {
        if insensitive {
            column.name.eq_ignore_ascii_case(&name.name)
        } else {
            column.name == name.name
        }
    });
    match (matches.next(), matches.next()) {
        (Some((position, _)), None) => Some(position),
        _ => None,
    }
}
