use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use bookie_app_services::operation_control::bounded_with;
use bookie_app_services::statement_plan::script_statements;
use bookie_core::sql_syntax::SqlGrammar;
use bookie_core::{QueryResultBudget, Value};
use bookie_gpui::live::{LiveSession, LiveTable, QueryFailure};
use bookie_gpui::source::ResultSource;
use tokio_util::sync::CancellationToken;

const MAX_SCRIPT_STATEMENTS: usize = 64;

pub(crate) struct Execution {
    statements: Vec<String>,
    params: Vec<Value>,
    schema: Option<String>,
}

pub(crate) struct Outcome {
    pub sql: String,
    pub elapsed: Duration,
    pub result: Result<Vec<LiveTable>, QueryFailure>,
}

impl Execution {
    pub fn plan(sql: &str, params: Vec<Value>, driver: &str, schema: &str) -> Result<Self, String> {
        let schema = bookie_app_services::schema_context::schema_context_for_run(driver, schema, true)
            .map_err(|_| "The selected schema is invalid or unsupported on this connection".to_owned())?;
        let Some(grammar) = bookie_app_services::result_editability::grammar_for_driver(driver) else {
            return Ok(Self {
                statements: vec![sql.to_owned()],
                params,
                schema,
            });
        };
        let plan = script_statements(sql, driver)?;
        if plan.statements.is_empty() {
            return Err("Type a statement first".into());
        }
        if plan.settings_check.is_some() {
            return Err("Statement boundaries depend on the server SQL mode. Run these statements separately".into());
        }
        if plan.statements.len() > MAX_SCRIPT_STATEMENTS {
            return Err(format!("Run at most {MAX_SCRIPT_STATEMENTS} statements together"));
        }
        if plan.statements.len() > 1 && !params.is_empty() {
            return Err("Run parameterized statements one at a time".into());
        }
        let keep_original = plan.source_ranges.first().is_some_and(|range| {
            let directive = match grammar {
                SqlGrammar::MsSql => "GO",
                SqlGrammar::MySql => "DELIMITER",
                _ => return true,
            };
            ![&sql[..range.start], &sql[range.end..]]
                .iter()
                .flat_map(|outside| outside.lines())
                .filter_map(|line| line.split_whitespace().next())
                .any(|word| word.eq_ignore_ascii_case(directive))
        });
        let statements = if plan.statements.len() == 1 && keep_original {
            vec![sql.to_owned()]
        } else {
            plan.statements
        };
        Ok(Self {
            statements,
            params,
            schema,
        })
    }

    pub fn run(self, session: &Arc<LiveSession>, cancel: CancellationToken) -> Vec<Outcome> {
        let control = bounded_with(30, cancel);
        let mut outcomes = Vec::new();
        let mut budget = QueryResultBudget::default();
        let needs_session = self.schema.is_some() || self.statements.len() > 1;
        let shared = if needs_session {
            match session.open_editor_session(&control) {
                Ok(shared) => Some(shared),
                Err(error) => return vec![failure(&self.statements[0], error)],
            }
        } else {
            None
        };
        if let Some(schema) = self.schema.as_deref()
            && let Err(mut error) = session.run_editor_batch(shared.as_ref(), schema, &[], &control, &mut budget)
        {
            error.message = format!("Could not apply the selected schema or database: {}", error.message);
            error.position = None;
            outcomes.push(failure(&self.statements[0], error));
        }
        let total = self.statements.len();
        if outcomes.is_empty() {
            for (index, sql) in self.statements.into_iter().enumerate() {
                let (started, clock) = (Instant::now(), SystemTime::now());
                let result = session.run_editor_batch(shared.as_ref(), &sql, &self.params, &control, &mut budget);
                let elapsed = started.elapsed();
                let rows = result
                    .as_ref()
                    .ok()
                    .and_then(|tables| i64::try_from(tables.iter().map(ResultSource::total).sum::<u64>()).ok());
                let error = result.as_ref().err().map(|error| error.message.clone());
                session.record_history(&sql, clock, elapsed, rows, error);
                let failed = result.is_err();
                let result = result.map_err(|error| name_failed_statement(error, index, total));
                outcomes.push(Outcome { sql, elapsed, result });
                if failed {
                    break;
                }
            }
        }
        if let Some(shared) = shared
            && let Err(error) = session.close_editor_session(shared)
        {
            outcomes.push(failure("Close query session", error));
        }
        outcomes
    }
}

fn name_failed_statement(mut error: QueryFailure, index: usize, total: usize) -> QueryFailure {
    if total > 1 {
        error.message = format!("Statement {} of {total} failed: {}", index + 1, error.message);
    }
    error
}

fn failure(sql: &str, error: QueryFailure) -> Outcome {
    Outcome {
        sql: sql.to_owned(),
        elapsed: Duration::ZERO,
        result: Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mssql_runs_one_batch_without_sending_the_go_directive() {
        let sql = "DECLARE @x INT = 7; SELECT @x;\nGO";
        let plan = Execution::plan(sql, vec![], "mssql", "").unwrap();
        assert_eq!(plan.statements, ["DECLARE @x INT = 7; SELECT @x;"]);
    }

    #[test]
    fn a_selected_schema_uses_the_quoted_session_statement() {
        let plan = Execution::plan("SELECT * FROM invoices", vec![], "postgres", "accounts 2026").unwrap();
        assert_eq!(plan.schema.as_deref(), Some("SET search_path TO \"accounts 2026\""));
        assert_eq!(plan.statements, ["SELECT * FROM invoices"]);
    }
}
