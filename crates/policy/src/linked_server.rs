use std::ops::ControlFlow;

use sqlparser::ast::{ObjectName, Statement, visit_relations};

const LINKED_SERVER_NAME_PARTS: usize = 4;

pub(crate) fn names_a_linked_server_table(driver_id: &str, statements: &[Statement]) -> bool {
    driver_id == "mssql"
        && statements.iter().any(|statement| {
            visit_relations(statement, |relation: &ObjectName| {
                if relation.0.len() >= LINKED_SERVER_NAME_PARTS {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .is_break()
        })
}
