use sqlparser::ast::Statement;

use crate::classify::{StatementClass, StatementDetails, classify_statement_details};
use crate::effects::Effects;

pub(crate) fn is_read_only_show(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::ShowVariable { .. }
            | Statement::ShowStatus { .. }
            | Statement::ShowVariables { .. }
            | Statement::ShowCreate { .. }
            | Statement::ShowColumns { .. }
            | Statement::ShowCatalogs { .. }
            | Statement::ShowDatabases { .. }
            | Statement::ShowProcessList { .. }
            | Statement::ShowSchemas { .. }
            | Statement::ShowCharset(_)
            | Statement::ShowObjects(_)
            | Statement::ShowTables { .. }
            | Statement::ShowViews { .. }
            | Statement::ShowCollation { .. }
    )
}

pub(crate) fn details(stmt: &Statement) -> StatementDetails {
    if !is_read_only_show(stmt) {
        return classify_statement_details(stmt);
    }
    StatementDetails {
        effects: Effects::EMPTY,
        class: StatementClass::Select,
        tables: Vec::new(),
        has_where: true,
        contains_unscoped_dml: false,
    }
}
