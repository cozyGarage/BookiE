use std::future::Future;

use crate::services::operation_control::OperationControl;
use bookie_app_services::stream_result_spool::StreamResultSpoolManager;
use bookie_core::sql_syntax::script::LexicalSettings;

use super::*;

pub(super) async fn run_tracked_statements<S, F, Fut>(
    run: StatementRun<'_, S>,
    stream_spool_manager: StreamResultSpoolManager,
    mut stream_update: F,
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
    let (mut out, mut skip) = (Vec::with_capacity(statements.len()), None);
    for (statement_index, sql) in statements.into_iter().enumerate() {
        match await_tracked_statement(
            &target,
            sql,
            statement_index,
            driver_id,
            parameter_values,
            control,
            single_statement,
            stops_on_error,
            last_index,
            settings_check,
            skip.take(),
            stream_spool_manager.clone(),
            &succeeded,
            &mut stream_update,
            &mut out,
        )
        .await
        {
            Ok(next) => skip = next,
            Err(result) => return result,
        }
    }
    ScriptRunResult::Completed(out)
}

async fn await_tracked_statement<S, F, Fut>(
    target: &super::session_mode::StatementTarget,
    sql: String,
    statement_index: usize,
    driver_id: &str,
    parameter_values: &std::collections::HashMap<String, bookie_core::Value>,
    control: &OperationControl,
    single_statement: bool,
    stops_on_error: bool,
    last_index: usize,
    settings_check: Option<LexicalSettings>,
    skip: Option<Option<String>>,
    spool_manager: StreamResultSpoolManager,
    succeeded: &S,
    stream_update: &mut F,
    out: &mut Vec<StatementOutcome>,
) -> Result<Option<Option<String>>, ScriptRunResult>
where
    S: Fn(&str),
    F: FnMut(StreamUpdate) -> Fut,
    Fut: Future<Output = bool>,
{
    let identity = statement_identity(statement_index, &sql);
    let preview = sql_preview(&sql);
    if let Some(reason) = skip {
        out.push(skipped_outcome(identity, preview, sql, reason));
        return Ok(None);
    }
    let (started, waited_before) = (std::time::Instant::now(), crate::services::approval_wait::waited());
    let bound = match bind_statement_for_run(
        &sql,
        driver_id,
        parameter_values,
        identity,
        preview.clone(),
        sql.clone(),
    ) {
        Ok(bound) => bound,
        Err(reason) => {
            out.push(*reason);
            return Ok(stops_on_error.then_some(None));
        }
    };
    let kind = match run_statement(
        StreamAttempt {
            target,
            single_statement,
            driver_id,
            bound_sql: &bound.sql,
            bound_params: &bound.values,
            identity,
            preview: preview.clone(),
            sql: &sql,
            started,
            control,
            spool_manager,
        },
        stream_update,
        succeeded,
    )
    .await
    {
        Ok(kind) => kind,
        Err(Halt::Finished(result)) => return Err(result),
        Err(Halt::Failed(outcome)) => {
            out.push(*outcome);
            return Err(ScriptRunResult::Completed(std::mem::take(out)));
        }
    };
    let mut next = (stops_on_error && matches!(kind, StatementOutcomeKind::Error(_))).then_some(None);
    out.push(statement_outcome(identity, preview, sql, started, waited_before, kind));
    if next.is_none() && statement_index < last_index {
        match lexical_settings_change(target, settings_check, control).await {
            Ok(change) => next = change.map(Some),
            Err(result) => return Err(result),
        }
    }
    Ok(next)
}
