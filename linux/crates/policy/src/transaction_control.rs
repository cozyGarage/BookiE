use sqlparser::ast::Statement;
use sqlparser::parser::Parser;

use crate::classify::dialect_for;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionControl {
    Begin,
    Commit { chain: bool },
    Rollback { chain: bool },
}

pub fn transaction_control(sql: &str, driver_id: &str) -> Option<TransactionControl> {
    let dialect = dialect_for(driver_id);
    let statements = Parser::parse_sql(dialect.as_ref(), sql.trim()).ok()?;
    let [statement] = statements.as_slice() else {
        return None;
    };
    match statement {
        Statement::StartTransaction { statements, .. } if statements.is_empty() => Some(TransactionControl::Begin),
        Statement::Commit { chain, .. } => Some(TransactionControl::Commit { chain: *chain }),
        Statement::Rollback { chain, savepoint: None } => Some(TransactionControl::Rollback { chain: *chain }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_transaction_statement_is_recognised_per_dialect() {
        let cases = [
            ("BEGIN", "postgres", Some(TransactionControl::Begin)),
            ("START TRANSACTION", "mysql", Some(TransactionControl::Begin)),
            ("BEGIN TRANSACTION", "mssql", Some(TransactionControl::Begin)),
            ("COMMIT", "sqlite", Some(TransactionControl::Commit { chain: false })),
            ("END", "postgres", Some(TransactionControl::Commit { chain: false })),
            (
                "COMMIT AND CHAIN",
                "postgres",
                Some(TransactionControl::Commit { chain: true }),
            ),
            ("ROLLBACK", "mysql", Some(TransactionControl::Rollback { chain: false })),
            ("ROLLBACK TO SAVEPOINT s", "postgres", None),
            ("SAVEPOINT s", "postgres", None),
            ("SELECT 1", "postgres", None),
            ("BEGIN; COMMIT", "postgres", None),
            ("not sql at all", "postgres", None),
        ];
        for (sql, driver, expected) in cases {
            assert_eq!(transaction_control(sql, driver), expected, "{driver}: {sql}");
        }
    }
}
