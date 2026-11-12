use std::future::Future;

use bookie_app_services::stream_result_spool::StreamResultSpoolManager;
use bookie_core::OperationControl;
use bookie_core::sql_syntax::script::LexicalSettings;

use super::*;

struct TrackedRunContext<'a, S, F> {
    target: &'a crate::ui::editor::session_mode::StatementTarget,
    driver_id: &'a str,
    parameter_values: &'a std::collections::HashMap<String, bookie_core::Value>,
    control: &'a OperationControl,
    single_statement: bool,
    stops_on_error: bool,
    last_index: usize,
    settings_check: Option<LexicalSettings>,
    spool_manager: StreamResultSpoolManager,
    succeeded: &'a S,
    stream_update: F,
    outcomes: Vec<StatementOutcome>,
}

pub(super) async fn run_tracked_statements<S, F, Fut>(
    run: StatementRun<'_, S>,
    stream_spool_manager: StreamResultSpoolManager,
    stream_update: F,
) -> ScriptRunResult
where
    S: Fn(&str),
    F: FnMut(StreamUpdate) -> Fut,
    Fut: Future<Output = bool>,
{
    let StatementRun {
        target,
        statements,
        driver_id,
        parameter_values,
        control,
        error_policy,
        settings_check,
        succeeded,
    } = run;
    let (single_statement, stops_on_error, last_index) = run_options(statements.len(), error_policy);
    let mut context = TrackedRunContext {
        target: &target,
        driver_id,
        parameter_values,
        control,
        single_statement,
        stops_on_error,
        last_index,
        settings_check,
        spool_manager: stream_spool_manager,
        succeeded: &succeeded,
        stream_update,
        outcomes: Vec::with_capacity(statements.len()),
    };
    let mut skip = None;
    for (statement_index, sql) in statements.into_iter().enumerate() {
        match await_tracked_statement(&mut context, sql, statement_index, skip.take()).await {
            Ok(next) => skip = next,
            Err(result) => return result,
        }
    }
    ScriptRunResult::Completed(context.outcomes)
}

async fn await_tracked_statement<S, F, Fut>(
    context: &mut TrackedRunContext<'_, S, F>,
    sql: String,
    statement_index: usize,
    skip: Option<Option<String>>,
) -> Result<Option<Option<String>>, ScriptRunResult>
where
    S: Fn(&str),
    F: FnMut(StreamUpdate) -> Fut,
    Fut: Future<Output = bool>,
{
    let identity = statement_identity(statement_index, &sql);
    let preview = sql_preview(&sql);
    if let Some(reason) = skip {
        context.outcomes.push(skipped_outcome(identity, preview, sql, reason));
        return Ok(None);
    }
    let (started, waited_before) = (std::time::Instant::now(), crate::services::approval_wait::waited());
    let bound = match bind_statement_for_run(
        &sql,
        context.driver_id,
        context.parameter_values,
        identity,
        preview.clone(),
        sql.clone(),
    ) {
        Ok(bound) => bound,
        Err(reason) => {
            context.outcomes.push(*reason);
            return Ok(context.stops_on_error.then_some(None));
        }
    };
    let attempt = StatementAttempt {
        identity,
        preview: &preview,
        sql: &sql,
        started,
        bound: &bound,
    };
    let (kind, messages) = match run_bound_statement(context, attempt).await {
        Ok(run) => run,
        Err(Halt::Finished(result)) => return Err(result),
        Err(Halt::Failed(outcome)) => {
            context.outcomes.push(*outcome);
            return Err(ScriptRunResult::Completed(std::mem::take(&mut context.outcomes)));
        }
    };
    let mut next = (context.stops_on_error && matches!(kind, StatementOutcomeKind::Error(_))).then_some(None);
    context.outcomes.push(statement_outcome(
        identity,
        preview,
        sql,
        started,
        waited_before,
        kind,
        messages,
    ));
    if next.is_none() && statement_index < context.last_index {
        match lexical_settings_change(context.target, context.settings_check, context.control).await {
            Ok(change) => next = change.map(Some),
            Err(result) => return Err(result),
        }
    }
    Ok(next)
}

struct StatementAttempt<'a> {
    identity: StatementIdentity,
    preview: &'a str,
    sql: &'a str,
    started: std::time::Instant,
    bound: &'a crate::services::query_parameters::BoundStatement,
}

async fn run_bound_statement<S, F, Fut>(
    context: &mut TrackedRunContext<'_, S, F>,
    attempt: StatementAttempt<'_>,
) -> Result<(StatementOutcomeKind, ServerMessages), Halt>
where
    S: Fn(&str),
    F: FnMut(StreamUpdate) -> Fut,
    Fut: Future<Output = bool>,
{
    run_statement(
        StreamAttempt {
            target: context.target,
            single_statement: context.single_statement,
            driver_id: context.driver_id,
            bound_sql: &attempt.bound.sql,
            bound_params: &attempt.bound.values,
            identity: attempt.identity,
            preview: attempt.preview.to_owned(),
            sql: attempt.sql,
            started: attempt.started,
            control: context.control,
            spool_manager: context.spool_manager.clone(),
        },
        &mut context.stream_update,
        context.succeeded,
    )
    .await
}
