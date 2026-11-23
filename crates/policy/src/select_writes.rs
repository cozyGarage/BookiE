use std::ops::ControlFlow;

use sqlparser::ast::{
    Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, JoinConstraint, JoinOperator, ObjectName, Query,
    Select, SelectItem, SetExpr, TableFactor, Visit, Visitor,
};

use sqlparser::dialect::Dialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer};

use crate::admin_functions::is_administrative_function_name;
use crate::classify::{StatementClass, StatementFacts, classify_query, table_factor_names};
use crate::effects::Effects;

pub(crate) struct SelectFacts {
    pub(crate) administrative: bool,
    pub(crate) writes: bool,
    pub(crate) tables: Vec<String>,
}

#[derive(Clone, Copy, Default)]
struct ExprFacts {
    administrative: bool,
    writes: bool,
}

impl ExprFacts {
    fn or(self, other: Self) -> Self {
        Self {
            administrative: self.administrative || other.administrative,
            writes: self.writes || other.writes,
        }
    }

    fn writes(writes: bool) -> Self {
        Self {
            administrative: false,
            writes,
        }
    }
}

pub(crate) fn select_facts(select: &Select) -> SelectFacts {
    let mut facts = ExprFacts::writes(select.into.is_some());
    for item in &select.projection {
        if let SelectItem::ExprWithAlias { expr, .. } | SelectItem::UnnamedExpr(expr) = item {
            facts = facts.or(expr_facts(expr, true));
        }
    }
    let mut tables = Vec::new();
    for table in &select.from {
        for factor in std::iter::once(&table.relation).chain(table.joins.iter().map(|join| &join.relation)) {
            let TableFactor::Derived { subquery, .. } = factor else {
                tables.extend(table_factor_names(factor));
                continue;
            };
            let inner = classify_query(subquery);
            facts.writes |= inner.class.is_write();
            tables.extend(inner.tables);
        }
        for constraint in table
            .joins
            .iter()
            .filter_map(|join| join_constraint(&join.join_operator))
        {
            facts.writes |= expr_writes(constraint);
        }
    }
    for predicate in select.selection.iter().chain(&select.having) {
        facts.writes |= expr_writes(predicate);
    }
    SelectFacts {
        administrative: facts.administrative,
        writes: facts.writes,
        tables,
    }
}

pub(crate) fn with_locking_backstop(mut facts: StatementFacts, sql: &str, dialect: &dyn Dialect) -> StatementFacts {
    if facts.class == StatementClass::Select && has_locking_clause(sql, dialect) {
        facts.class = StatementClass::Other;
        facts.effects = facts.effects.union(Effects::WRITES_ROWS);
    }
    facts
}

fn has_locking_clause(sql: &str, dialect: &dyn Dialect) -> bool {
    let Ok(tokens) = Tokenizer::new(dialect, sql).tokenize() else {
        return true;
    };
    let keywords: Vec<Option<Keyword>> = tokens
        .iter()
        .filter(|token| !matches!(token, Token::Whitespace(_)))
        .map(|token| match token {
            Token::Word(word) if word.quote_style.is_none() => Some(word.keyword),
            _ => None,
        })
        .collect();
    keywords.windows(2).enumerate().any(|(at, pair)| match pair {
        [Some(Keyword::FOR), Some(Keyword::UPDATE | Keyword::SHARE)] => true,
        [Some(Keyword::FOR), Some(Keyword::NO | Keyword::KEY)] => matches!(
            keywords.get(at + 2),
            Some(Some(Keyword::KEY | Keyword::SHARE | Keyword::UPDATE))
        ),
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

fn query_facts(query: &Query, projected: bool) -> ExprFacts {
    let class = classify_query(query).class;
    ExprFacts {
        administrative: projected && class == StatementClass::Administrative,
        writes: class.is_write(),
    }
}

fn expr_facts(expr: &Expr, projected: bool) -> ExprFacts {
    match expr {
        Expr::Subquery(query) => query_facts(query, projected),
        Expr::BinaryOp { left, right, .. } => expr_facts(left, projected).or(expr_facts(right, projected)),
        Expr::UnaryOp { expr, .. } | Expr::Nested(expr) => expr_facts(expr, projected),
        Expr::Function(function) => function_facts(function, projected),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => operand
            .iter()
            .chain(else_result)
            .map(|expr| expr_facts(expr, projected))
            .chain(conditions.iter().flat_map(|case| {
                [
                    expr_facts(&case.condition, projected),
                    expr_facts(&case.result, projected),
                ]
            }))
            .fold(ExprFacts::default(), ExprFacts::or),
        _ => ExprFacts::writes(expr_writes(expr)),
    }
}

fn expr_writes(expr: &Expr) -> bool {
    match expr {
        Expr::Subquery(_)
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::Nested(_)
        | Expr::Function(_)
        | Expr::Case { .. } => expr_facts(expr, false).writes,
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
        _ => nested_query_writes(expr),
    }
}

fn function_facts(function: &Function, projected: bool) -> ExprFacts {
    let named = ExprFacts {
        administrative: projected && is_administrative_function(&function.name),
        writes: false,
    };
    let arguments = match &function.args {
        FunctionArguments::Subquery(query) => return named.or(query_facts(query, projected)),
        FunctionArguments::List(list) => &list.args,
        FunctionArguments::None => return named,
    };
    let filter = ExprFacts::writes(function.filter.as_deref().is_some_and(expr_writes));
    arguments
        .iter()
        .filter_map(|argument| match argument {
            FunctionArg::Named { arg, .. } | FunctionArg::ExprNamed { arg, .. } | FunctionArg::Unnamed(arg) => {
                match arg {
                    FunctionArgExpr::Expr(expr) => Some(expr_facts(expr, projected)),
                    _ => None,
                }
            }
        })
        .fold(named.or(filter), ExprFacts::or)
}

fn is_administrative_function(name: &ObjectName) -> bool {
    is_administrative_function_name(&name.to_string())
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

#[cfg(test)]
mod tests {
    use crate::classify::{StatementClass, classify};

    #[test]
    fn select_into_creates_a_table_and_is_a_write() {
        for (sql, driver) in [
            ("SELECT * INTO backup FROM accounts", "postgres"),
            ("SELECT id INTO TEMP scratch FROM accounts", "postgres"),
            ("SELECT * INTO backup FROM accounts", "mssql"),
        ] {
            let facts = classify(sql, driver);
            assert!(facts.writes(), "{sql} must be classified as a write");
            assert_ne!(facts.class, StatementClass::Select, "{sql}");
        }
        assert!(!classify("SELECT * FROM accounts", "postgres").writes());
    }

    #[test]
    fn locking_reads_are_writes_because_a_read_only_session_refuses_them() {
        for sql in [
            "SELECT * FROM users WHERE id = 1 FOR UPDATE",
            "SELECT * FROM users FOR SHARE",
            "SELECT * FROM users FOR NO KEY UPDATE SKIP LOCKED",
            "WITH u AS (SELECT * FROM users FOR UPDATE) SELECT * FROM u",
            "SELECT * FROM (SELECT id FROM users FOR UPDATE) AS s",
        ] {
            assert!(classify(sql, "postgres").writes(), "{sql}");
        }
    }

    #[test]
    fn a_locking_read_nested_anywhere_in_a_select_is_a_write() {
        for (sql, driver) in [
            ("SELECT 1 WHERE EXISTS (SELECT 1 FROM accounts FOR UPDATE)", "postgres"),
            (
                "SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM accounts FOR UPDATE)",
                "postgres",
            ),
            (
                "SELECT * FROM t WHERE id IN (SELECT id FROM accounts FOR UPDATE)",
                "postgres",
            ),
            (
                "SELECT * FROM t WHERE id = (SELECT id FROM accounts LIMIT 1 FOR UPDATE)",
                "postgres",
            ),
            ("SELECT coalesce((SELECT id FROM accounts FOR UPDATE), 0)", "postgres"),
            ("SELECT CAST((SELECT id FROM accounts FOR UPDATE) AS text)", "postgres"),
            (
                "SELECT count(*) FROM t GROUP BY a HAVING max(a) > (SELECT 1 FROM accounts FOR UPDATE)",
                "postgres",
            ),
            (
                "SELECT * FROM t WHERE (SELECT 1 FROM accounts FOR UPDATE) IS NOT NULL",
                "postgres",
            ),
            (
                "SELECT * FROM t WHERE a BETWEEN 0 AND (SELECT 1 FROM accounts FOR UPDATE)",
                "postgres",
            ),
            (
                "SELECT * FROM t WHERE a IN (1, (SELECT 1 FROM accounts FOR UPDATE))",
                "postgres",
            ),
            (
                "SELECT * FROM t JOIN u ON u.id = (SELECT 1 FROM accounts FOR UPDATE)",
                "postgres",
            ),
            (
                "SELECT 1 FROM dual WHERE EXISTS (SELECT 1 FROM accounts FOR UPDATE)",
                "mysql",
            ),
            (
                "SELECT * FROM t WHERE id = ANY (SELECT id FROM accounts FOR UPDATE)",
                "postgres",
            ),
            (
                "SELECT * FROM t ORDER BY (SELECT 1 FROM accounts FOR UPDATE)",
                "postgres",
            ),
            (
                "SELECT a.id FROM a STRAIGHT_JOIN b ON b.id = (SELECT 1 FROM accounts FOR UPDATE)",
                "mysql",
            ),
            ("SELECT CONVERT((SELECT id FROM accounts FOR UPDATE), CHAR)", "mysql"),
            (
                "SELECT * FROM t WHERE id = ANY (SELECT id FROM accounts FOR NO KEY UPDATE)",
                "postgres",
            ),
            (
                "SELECT * FROM t WHERE id IN (SELECT id FROM accounts FOR KEY SHARE)",
                "postgres",
            ),
        ] {
            let facts = classify(sql, driver);
            assert!(facts.writes(), "{driver}: {sql}");
            assert_ne!(facts.class, StatementClass::Select, "{driver}: {sql}");
        }
        assert!(
            classify(
                "SELECT 1 WHERE EXISTS (SELECT 1 FROM accounts LOCK IN SHARE MODE)",
                "mysql"
            )
            .writes(),
            "an unparseable locking read still fails closed"
        );
        for sql in [
            "SELECT 'FOR UPDATE' AS note",
            "SELECT 1 /* FOR UPDATE */",
            "SELECT substring(name FROM 1 FOR 3) FROM t",
            "SELECT 1 WHERE EXISTS (SELECT 1 FROM accounts)",
            "SELECT coalesce((SELECT id FROM accounts LIMIT 1), 0)",
            "SELECT * FROM t WHERE id IN (SELECT id FROM accounts)",
        ] {
            assert!(!classify(sql, "postgres").writes(), "{sql}");
        }
    }
}
