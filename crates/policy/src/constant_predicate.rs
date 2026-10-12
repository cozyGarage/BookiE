use sqlparser::ast::{BinaryOperator, Expr, UnaryOperator, Value};

pub(crate) fn is_constant_true(expr: &Expr) -> bool {
    match expr {
        Expr::Value(value) => matches!(value.value, Value::Boolean(true)),
        Expr::Nested(inner) => is_constant_true(inner),
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            expr,
        } => is_constant_false(expr),
        Expr::BinaryOp {
            left,
            op: BinaryOperator::Eq,
            right,
        } => same_literal(left, right),
        Expr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => is_constant_true(left) && is_constant_true(right),
        Expr::BinaryOp {
            left,
            op: BinaryOperator::Or,
            right,
        } => is_constant_true(left) || is_constant_true(right),
        _ => false,
    }
}

fn is_constant_false(expr: &Expr) -> bool {
    match expr {
        Expr::Value(value) => matches!(value.value, Value::Boolean(false)),
        Expr::Nested(inner) => is_constant_false(inner),
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            expr,
        } => is_constant_true(expr),
        _ => false,
    }
}

fn same_literal(left: &Expr, right: &Expr) -> bool {
    match (left, right) {
        (Expr::Value(a), Expr::Value(b)) => match (&a.value, &b.value) {
            (Value::Number(a, _), Value::Number(b, _)) => a == b,
            (Value::SingleQuotedString(a), Value::SingleQuotedString(b)) => a == b,
            _ => false,
        },
        (Expr::Nested(a), b) | (b, Expr::Nested(a)) => same_literal(a, b),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::classify::classify;

    fn unscoped(sql: &str) -> bool {
        classify(sql, "postgres").contains_unscoped_dml
    }

    #[test]
    fn a_constant_true_predicate_is_unscoped() {
        for sql in [
            "DELETE FROM t WHERE true",
            "DELETE FROM t WHERE 1=1",
            "DELETE FROM t WHERE (1 = 1)",
            "DELETE FROM t WHERE 'a' = 'a'",
            "DELETE FROM t WHERE id = 1 OR true",
            "DELETE FROM t WHERE NOT false",
            "DELETE FROM t WHERE true AND 1 = 1",
            "UPDATE t SET a = 1 WHERE true",
            "UPDATE t SET a = 1 WHERE 1=1",
        ] {
            assert!(unscoped(sql), "{sql}");
        }
    }

    #[test]
    fn a_real_predicate_is_scoped() {
        for sql in [
            "DELETE FROM t WHERE id = 1",
            "DELETE FROM t WHERE 1 = 2",
            "DELETE FROM t WHERE false",
            "DELETE FROM t WHERE id = 1 AND true",
            "UPDATE t SET a = 1 WHERE id = 1",
        ] {
            assert!(!unscoped(sql), "{sql}");
        }
    }

    #[test]
    fn a_statement_without_a_predicate_is_still_unscoped() {
        assert!(unscoped("DELETE FROM t"));
        assert!(unscoped("UPDATE t SET a = 1"));
    }
}
