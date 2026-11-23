use std::ops::ControlFlow;

use sqlparser::ast::{
    CopySource, CopyTarget, Cte, Delete, Expr, FromTable, Function, FunctionArg, FunctionArgExpr, FunctionArguments,
    GrantObjects, Insert, JoinConstraint, JoinOperator, ObjectName, Query, Select, SelectItem, SetExpr, Statement,
    TableFactor, TableObject, TableWithJoins, Update, UpdateTableFromKind, UtilityOption, Visit, Visitor,
};

use crate::admin_functions::is_administrative_function_name;
use crate::classify::{StatementClass, StatementDetails};
use crate::constant_predicate::is_constant_true;
use crate::effects::Effects;
use crate::effects_classification::{statement_effects, truncate_facts};
use crate::explain_options::analyze_option_enabled;

pub(super) fn classify_statement(stmt: &Statement) -> StatementDetails {
    let mut facts = if crate::show_statements::is_read_only_show(stmt) {
        StatementDetails {
            effects: Effects::EMPTY,
            class: StatementClass::Select,
            tables: Vec::new(),
            has_where: true,
            contains_unscoped_dml: false,
        }
    } else {
        classify_statement_details(stmt)
    };
    facts.effects = facts.effects.union(statement_effects(stmt, &facts));
    facts
}

fn classify_statement_details(stmt: &Statement) -> StatementDetails {
    match stmt {
        Statement::Query(q) => classify_query(q),
        Statement::Insert(insert) => classify_insert(insert),
        Statement::Update(update) => StatementDetails {
            effects: Effects::EMPTY,
            class: StatementClass::Update,
            tables: update_table_names(update),
            has_where: update.selection.is_some(),
            contains_unscoped_dml: update.selection.as_ref().is_none_or(is_constant_true),
        },
        Statement::Delete(delete) => classify_delete(delete),
        Statement::CreateTable(c) => ddl_on(object_name_strings(&c.name)),
        Statement::CreateView(view) => ddl_on(object_name_strings(&view.name)),
        Statement::CreateIndex(c) => ddl_on(match &c.name {
            Some(n) => object_name_strings(n),
            None => object_name_strings(&c.table_name),
        }),
        Statement::AlterTable(alter) => ddl_on(object_name_strings(&alter.name)),
        Statement::RenameTable(renames) => ddl_on(
            renames
                .iter()
                .flat_map(|rename| object_name_strings(&rename.old_name))
                .collect(),
        ),
        Statement::Drop { names, .. } => ddl_on(names.iter().flat_map(object_name_strings).collect()),
        Statement::Truncate(truncate) => truncate_facts(truncate.table_names.iter().map(|table| &table.name)),
        Statement::CreateVirtualTable { .. }
        | Statement::CreateRole { .. }
        | Statement::CreateSecret { .. }
        | Statement::CreateServer(_)
        | Statement::CreatePolicy { .. }
        | Statement::CreateConnector(_)
        | Statement::AlterIndex { .. }
        | Statement::AlterView { .. }
        | Statement::AlterType(_)
        | Statement::AlterRole { .. }
        | Statement::AlterPolicy { .. }
        | Statement::AlterConnector { .. }
        | Statement::DropFunction { .. }
        | Statement::DropDomain(_)
        | Statement::DropProcedure { .. }
        | Statement::DropSecret { .. }
        | Statement::DropPolicy { .. }
        | Statement::DropConnector { .. }
        | Statement::CreateExtension { .. }
        | Statement::DropExtension { .. }
        | Statement::Comment { .. }
        | Statement::CreateSchema { .. }
        | Statement::CreateDatabase { .. }
        | Statement::CreateTrigger { .. }
        | Statement::DropTrigger { .. }
        | Statement::CreateProcedure { .. }
        | Statement::CreateMacro { .. }
        | Statement::CreateStage { .. }
        | Statement::Deny(_)
        | Statement::CreateSequence { .. }
        | Statement::CreateDomain(_)
        | Statement::CreateType { .. } => ddl_facts(Vec::new()),
        Statement::CreateFunction(f) => ddl_facts(object_name_strings(&f.name)),
        Statement::Grant(grant) => ddl_facts(grant.objects.as_ref().map(grant_object_names).unwrap_or_default()),
        Statement::Revoke(revoke) => ddl_facts(revoke.objects.as_ref().map(grant_object_names).unwrap_or_default()),
        Statement::Merge(merge) => StatementDetails {
            effects: Effects::EMPTY,
            class: StatementClass::Other,
            tables: table_factor_names(&merge.table),
            has_where: true,
            contains_unscoped_dml: false,
        },
        Statement::Kill { .. } => StatementDetails {
            effects: Effects::EMPTY,
            class: StatementClass::Administrative,
            tables: Vec::new(),
            has_where: true,
            contains_unscoped_dml: false,
        },
        Statement::StartTransaction { .. } | Statement::Commit { .. } | Statement::Rollback { .. } => {
            StatementDetails {
                effects: Effects::EMPTY,
                class: StatementClass::Transaction,
                tables: Vec::new(),
                has_where: true,
                contains_unscoped_dml: false,
            }
        }
        Statement::Explain {
            analyze,
            statement,
            options,
            ..
        } => classify_explain(*analyze, statement, options.as_deref()),
        Statement::ExplainTable { table_name, .. } => StatementDetails {
            effects: Effects::EMPTY,
            class: StatementClass::Select,
            tables: object_name_strings(table_name),
            has_where: true,
            contains_unscoped_dml: false,
        },
        Statement::Copy { target, source, .. } => {
            let reaches_the_host = matches!(target, CopyTarget::File { .. } | CopyTarget::Program { .. });
            let tables = match source {
                CopySource::Table { table_name, .. } => object_name_strings(table_name),
                CopySource::Query(query) => classify_query(query).tables,
            };
            StatementDetails {
                effects: Effects::EMPTY,
                class: if reaches_the_host {
                    StatementClass::Administrative
                } else {
                    StatementClass::Other
                },
                tables,
                has_where: true,
                contains_unscoped_dml: false,
            }
        }
        _ => StatementDetails {
            effects: Effects::EMPTY,
            class: StatementClass::Other,
            tables: Vec::new(),
            has_where: true,
            contains_unscoped_dml: false,
        },
    }
}

fn classify_explain(analyze: bool, statement: &Statement, options: Option<&[UtilityOption]>) -> StatementDetails {
    if analyze || options.is_some_and(analyze_option_enabled) {
        return classify_statement(statement);
    }
    let inner = classify_statement(statement);
    StatementDetails {
        effects: Effects::EMPTY,
        class: StatementClass::Select,
        tables: inner.tables,
        has_where: true,
        contains_unscoped_dml: false,
    }
}

fn ddl_facts(tables: Vec<String>) -> StatementDetails {
    StatementDetails {
        effects: Effects::EMPTY,
        class: StatementClass::Ddl,
        tables,
        has_where: true,
        contains_unscoped_dml: false,
    }
}

fn classify_insert(insert: &Insert) -> StatementDetails {
    let tables = match &insert.table {
        TableObject::TableName(name) => object_name_strings(name),
        TableObject::TableFunction(f) => vec![f.name.to_string()],
        TableObject::TableQuery(query) => classify_query(query).tables,
    };
    StatementDetails {
        effects: Effects::EMPTY,
        class: StatementClass::Insert,
        tables,
        has_where: true,
        contains_unscoped_dml: false,
    }
}

fn classify_delete(delete: &Delete) -> StatementDetails {
    let mut tables = Vec::new();
    for t in &delete.tables {
        tables.extend(object_name_strings(t));
    }
    tables.extend(from_table_names(&delete.from));
    for using in delete.using.iter().flatten() {
        tables.extend(table_with_joins_names(using));
    }
    StatementDetails {
        effects: Effects::EMPTY,
        class: StatementClass::Delete,
        tables,
        has_where: delete.selection.is_some(),
        contains_unscoped_dml: delete.selection.as_ref().is_none_or(is_constant_true),
    }
}

fn from_table_names(from: &FromTable) -> Vec<String> {
    let list = match from {
        FromTable::WithFromKeyword(t) | FromTable::WithoutKeyword(t) => t,
    };
    list.iter().flat_map(table_with_joins_names).collect()
}

fn table_with_joins_names(table: &TableWithJoins) -> Vec<String> {
    let mut names = table_factor_names(&table.relation);
    for join in &table.joins {
        names.extend(table_factor_names(&join.relation));
    }
    names
}

fn update_table_names(update: &Update) -> Vec<String> {
    let mut names = table_with_joins_names(&update.table);
    let from = match &update.from {
        Some(UpdateTableFromKind::BeforeSet(from) | UpdateTableFromKind::AfterSet(from)) => from.as_slice(),
        None => &[],
    };
    for table in from {
        names.extend(table_with_joins_names(table));
    }
    names
}

fn classify_query(query: &Query) -> StatementDetails {
    let mut writes = false;
    let mut administrative = false;
    let mut contains_unscoped_dml = false;
    let mut effects = Effects::EMPTY;
    let mut tables = Vec::new();

    if let Some(with) = &query.with {
        for Cte { query: cte_q, .. } in &with.cte_tables {
            let inner = classify_query(cte_q);
            writes |= inner.class.is_write();
            administrative |= inner.class == StatementClass::Administrative;
            effects = effects.union(inner.effects);
            contains_unscoped_dml |= inner.contains_unscoped_dml;
            tables.extend(inner.tables);
        }
    }

    let body_facts = classify_set_expr(query.body.as_ref());
    writes |= body_facts.class.is_write() || !query.locks.is_empty();
    administrative |= body_facts.class == StatementClass::Administrative;
    effects = effects.union(body_facts.effects);
    contains_unscoped_dml |= body_facts.contains_unscoped_dml;
    effects = effects.union_if(Effects::WRITES_ROWS, !query.locks.is_empty());
    tables.extend(body_facts.tables);

    StatementDetails {
        effects,
        class: if administrative {
            StatementClass::Administrative
        } else if writes {
            StatementClass::Other
        } else {
            StatementClass::Select
        },
        tables,
        has_where: !contains_unscoped_dml,
        contains_unscoped_dml,
    }
}

fn classify_set_expr(body: &SetExpr) -> StatementDetails {
    match body {
        SetExpr::Insert(statement) | SetExpr::Update(statement) | SetExpr::Delete(statement) => {
            classify_statement(statement)
        }
        SetExpr::Query(query) => classify_query(query),
        SetExpr::SetOperation { left, right, .. } => {
            let left = classify_set_expr(left);
            let right = classify_set_expr(right);
            StatementDetails {
                class: if left.class == StatementClass::Administrative || right.class == StatementClass::Administrative
                {
                    StatementClass::Administrative
                } else if left.class.is_write() || right.class.is_write() {
                    StatementClass::Other
                } else {
                    StatementClass::Select
                },
                tables: left.tables.into_iter().chain(right.tables).collect(),
                has_where: left.has_where && right.has_where,
                effects: left.effects.union(right.effects),
                contains_unscoped_dml: left.contains_unscoped_dml || right.contains_unscoped_dml,
            }
        }
        _ => {
            let administrative = set_expr_administrative(body);
            let writes = set_expr_writes(body) || administrative;
            StatementDetails {
                effects: if administrative {
                    Effects::ADMIN
                } else if writes {
                    Effects::UNKNOWN
                } else {
                    Effects::READS
                },
                class: if administrative {
                    StatementClass::Administrative
                } else if writes {
                    StatementClass::Other
                } else {
                    StatementClass::Select
                },
                tables: set_expr_tables(body),
                has_where: true,
                contains_unscoped_dml: false,
            }
        }
    }
}

fn set_expr_administrative(body: &SetExpr) -> bool {
    match body {
        SetExpr::Query(query) => classify_query(query).class == StatementClass::Administrative,
        SetExpr::SetOperation { left, right, .. } => set_expr_administrative(left) || set_expr_administrative(right),
        SetExpr::Select(select) => select.projection.iter().any(|item| match item {
            SelectItem::ExprWithAlias { expr, .. } | SelectItem::UnnamedExpr(expr) => expr_is_administrative(expr),
            _ => false,
        }),
        _ => false,
    }
}

fn expr_is_administrative(expr: &Expr) -> bool {
    match expr {
        Expr::Function(function) => {
            is_administrative_function(&function.name) || function_arguments_are_administrative(&function.args)
        }
        Expr::Subquery(query) => classify_query(query).class == StatementClass::Administrative,
        Expr::BinaryOp { left, right, .. } => expr_is_administrative(left) || expr_is_administrative(right),
        Expr::UnaryOp { expr, .. } | Expr::Nested(expr) => expr_is_administrative(expr),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand.as_ref().is_some_and(|expr| expr_is_administrative(expr))
                || conditions
                    .iter()
                    .any(|case| expr_is_administrative(&case.condition) || expr_is_administrative(&case.result))
                || else_result.as_ref().is_some_and(|expr| expr_is_administrative(expr))
        }
        _ => false,
    }
}

fn is_administrative_function(name: &ObjectName) -> bool {
    is_administrative_function_name(&name.to_string())
}

fn function_arguments_are_administrative(arguments: &FunctionArguments) -> bool {
    match arguments {
        FunctionArguments::None => false,
        FunctionArguments::Subquery(query) => classify_query(query).class == StatementClass::Administrative,
        FunctionArguments::List(arguments) => arguments.args.iter().any(|argument| {
            let expression = match argument {
                FunctionArg::Named { arg, .. } | FunctionArg::ExprNamed { arg, .. } | FunctionArg::Unnamed(arg) => arg,
            };
            matches!(expression, FunctionArgExpr::Expr(expr) if expr_is_administrative(expr))
        }),
    }
}

fn set_expr_writes(body: &SetExpr) -> bool {
    match body {
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) | SetExpr::Merge(_) => true,
        SetExpr::Values(_) => false,
        SetExpr::Query(q) => classify_query(q).class.is_write(),
        SetExpr::SetOperation { left, right, .. } => set_expr_writes(left) || set_expr_writes(right),
        SetExpr::Select(select) => select_writes(select),
        SetExpr::Table(_) => false,
    }
}

fn set_expr_tables(body: &SetExpr) -> Vec<String> {
    match body {
        SetExpr::Select(select) => {
            let mut tables = Vec::new();
            for t in &select.from {
                tables.extend(table_factor_names(&t.relation));
                for join in &t.joins {
                    tables.extend(table_factor_names(&join.relation));
                }
            }
            tables
        }
        SetExpr::Query(q) => classify_query(q).tables,
        SetExpr::SetOperation { left, right, .. } => {
            let mut t = set_expr_tables(left);
            t.extend(set_expr_tables(right));
            t
        }
        SetExpr::Insert(s) | SetExpr::Update(s) | SetExpr::Delete(s) | SetExpr::Merge(s) => {
            classify_statement(s).tables
        }
        _ => Vec::new(),
    }
}

fn ddl_on(tables: Vec<String>) -> StatementDetails {
    StatementDetails {
        effects: Effects::EMPTY,
        class: StatementClass::Ddl,
        tables,
        has_where: true,
        contains_unscoped_dml: false,
    }
}

fn object_name_strings(name: &ObjectName) -> Vec<String> {
    vec![name.to_string()]
}

fn grant_object_names(objects: &GrantObjects) -> Vec<String> {
    match objects {
        GrantObjects::Tables(names) | GrantObjects::Views(names) | GrantObjects::Schemas(names) => {
            names.iter().flat_map(object_name_strings).collect()
        }
        GrantObjects::Databases(names) => names.iter().flat_map(object_name_strings).collect(),
        _ => Vec::new(),
    }
}

fn table_factor_names(factor: &TableFactor) -> Vec<String> {
    match factor {
        TableFactor::Table { name, .. } => object_name_strings(name),
        TableFactor::Derived { subquery, .. } => classify_query(subquery).tables,
        TableFactor::NestedJoin { table_with_joins, .. } => {
            let mut out = table_factor_names(&table_with_joins.relation);
            for j in &table_with_joins.joins {
                out.extend(table_factor_names(&j.relation));
            }
            out
        }
        _ => Vec::new(),
    }
}

fn select_writes(select: &Select) -> bool {
    select.into.is_some()
        || select.from.iter().any(|t| {
            std::iter::once(&t.relation)
                .chain(t.joins.iter().map(|j| &j.relation))
                .any(|factor| matches!(factor, TableFactor::Derived { subquery, .. } if classify_query(subquery).class.is_write()))
        })
        || select
            .from
            .iter()
            .flat_map(|t| t.joins.iter())
            .any(|join| join_constraint(&join.join_operator).is_some_and(expr_writes))
        || select.selection.as_ref().is_some_and(expr_writes)
        || select.having.as_ref().is_some_and(expr_writes)
        || select.projection.iter().any(|item| match item {
            SelectItem::ExprWithAlias { expr, .. } | SelectItem::UnnamedExpr(expr) => expr_writes(expr),
            _ => false,
        })
}

fn join_constraint(operator: &JoinOperator) -> Option<&Expr> {
    let constraint = match operator {
        JoinOperator::Join(c)
        | JoinOperator::Inner(c)
        | JoinOperator::Left(c)
        | JoinOperator::LeftOuter(c)
        | JoinOperator::Right(c)
        | JoinOperator::RightOuter(c)
        | JoinOperator::FullOuter(c)
        | JoinOperator::Semi(c)
        | JoinOperator::LeftSemi(c)
        | JoinOperator::RightSemi(c)
        | JoinOperator::Anti(c)
        | JoinOperator::LeftAnti(c)
        | JoinOperator::RightAnti(c) => c,
        _ => return None,
    };
    match constraint {
        JoinConstraint::On(expr) => Some(expr),
        _ => None,
    }
}

fn expr_writes(expr: &Expr) -> bool {
    match expr {
        Expr::Subquery(q) => classify_query(q).class.is_write(),
        Expr::BinaryOp { left, right, .. } => expr_writes(left) || expr_writes(right),
        Expr::UnaryOp { expr, .. } => expr_writes(expr),
        Expr::Nested(e) => expr_writes(e),
        Expr::Exists { subquery, .. } => classify_query(subquery).class.is_write(),
        Expr::InSubquery { expr, subquery, .. } => expr_writes(expr) || classify_query(subquery).class.is_write(),
        Expr::InList { expr, list, .. } => expr_writes(expr) || list.iter().any(expr_writes),
        Expr::Between { expr, low, high, .. } => expr_writes(expr) || expr_writes(low) || expr_writes(high),
        Expr::Cast { expr, .. }
        | Expr::IsNull(expr)
        | Expr::IsNotNull(expr)
        | Expr::IsTrue(expr)
        | Expr::IsFalse(expr)
        | Expr::IsNotTrue(expr)
        | Expr::IsNotFalse(expr)
        | Expr::Collate { expr, .. } => expr_writes(expr),
        Expr::Like { expr, pattern, .. }
        | Expr::ILike { expr, pattern, .. }
        | Expr::SimilarTo { expr, pattern, .. } => expr_writes(expr) || expr_writes(pattern),
        Expr::IsDistinctFrom(left, right) | Expr::IsNotDistinctFrom(left, right) => {
            expr_writes(left) || expr_writes(right)
        }
        Expr::Tuple(items) => items.iter().any(expr_writes),
        Expr::Function(function) => function_writes(function),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand.as_ref().is_some_and(|e| expr_writes(e))
                || conditions
                    .iter()
                    .any(|c| expr_writes(&c.condition) || expr_writes(&c.result))
                || else_result.as_ref().is_some_and(|e| expr_writes(e))
        }
        _ => nested_query_writes(expr),
    }
}

struct NestedQueryWrite;

impl Visitor for NestedQueryWrite {
    type Break = ();

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<()> {
        if set_expr_is_write_node(&query.body) || !query.locks.is_empty() {
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }
}

fn set_expr_is_write_node(body: &SetExpr) -> bool {
    match body {
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) | SetExpr::Merge(_) => true,
        SetExpr::Select(select) => select.into.is_some(),
        SetExpr::SetOperation { left, right, .. } => set_expr_is_write_node(left) || set_expr_is_write_node(right),
        SetExpr::Query(query) => set_expr_is_write_node(&query.body),
        SetExpr::Values(_) | SetExpr::Table(_) => false,
    }
}

fn nested_query_writes(expr: &Expr) -> bool {
    expr.visit(&mut NestedQueryWrite).is_break()
}

fn function_writes(function: &Function) -> bool {
    let arguments = match &function.args {
        FunctionArguments::Subquery(query) => return classify_query(query).class.is_write(),
        FunctionArguments::List(list) => &list.args,
        FunctionArguments::None => return false,
    };
    arguments.iter().any(|argument| {
        let value = match argument {
            FunctionArg::Named { arg, .. } | FunctionArg::ExprNamed { arg, .. } | FunctionArg::Unnamed(arg) => arg,
        };
        matches!(value, FunctionArgExpr::Expr(expr) if expr_writes(expr))
    }) || function.filter.as_deref().is_some_and(expr_writes)
}
