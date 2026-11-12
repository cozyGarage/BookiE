use std::future::Future;
use std::time::Duration;

use bookie_core::{ColumnInfo, Connection, DriverError, OperationControl, QueryResult, Value};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::{
    MAX_HITS_PER_TABLE, MAX_SEARCH_TABLES, PlanError, SearchLimits, SearchOptions, SearchProgress, SearchReport,
    SearchRequest, SearchTable, StopReason, TableHit, TableOutcome, TablePlan, TableStatus, TableTarget, plan_table,
    validate_term,
};

pub trait SearchSource {
    fn columns(
        &self,
        table: &TableTarget,
        control: &OperationControl,
    ) -> impl Future<Output = Result<Vec<ColumnInfo>, DriverError>> + Send;

    fn query(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> impl Future<Output = Result<QueryResult, DriverError>> + Send;
}

pub struct ConnectionSource<'a>(pub &'a dyn Connection);

impl SearchSource for ConnectionSource<'_> {
    async fn columns(&self, table: &TableTarget, control: &OperationControl) -> Result<Vec<ColumnInfo>, DriverError> {
        self.0
            .fetch_columns_controlled(table.schema.as_deref(), &table.name, control)
            .await
    }

    async fn query(&self, sql: &str, params: &[Value], control: &OperationControl) -> Result<QueryResult, DriverError> {
        self.0.query_params_controlled(sql, params, control).await
    }
}

enum Step {
    Done(TableOutcome),
    Stop(StopReason),
}

struct Run<'a, S> {
    source: &'a S,
    driver_id: &'a str,
    term: &'a str,
    options: SearchOptions,
    limits: SearchLimits,
    token: &'a CancellationToken,
    deadline: Instant,
}

pub async fn run_search(
    source: &impl SearchSource,
    request: SearchRequest<'_>,
    token: &CancellationToken,
    mut progress: impl FnMut(SearchProgress),
) -> Result<SearchReport, PlanError> {
    let term = validate_term(request.term)?;
    let mut report = SearchReport {
        tables_over_limit: request.tables.len().saturating_sub(MAX_SEARCH_TABLES),
        ..SearchReport::default()
    };
    let queue: Vec<TableTarget> = request.tables.into_iter().take(MAX_SEARCH_TABLES).collect();
    let total = queue.len();
    let run = Run {
        source,
        driver_id: request.driver_id,
        term,
        options: request.options,
        limits: request.limits,
        token,
        deadline: Instant::now() + request.limits.run_timeout,
    };
    for (index, target) in queue.into_iter().enumerate() {
        if let Some(reason) = run.stop_before() {
            report.stopped = Some(reason);
            report.tables_not_reached = total - index;
            return Ok(report);
        }
        progress(SearchProgress {
            searched: index,
            total,
            current: target.label(),
        });
        match run.search_one(target).await? {
            Step::Done(outcome) => report.tables.push(outcome),
            Step::Stop(reason) => {
                report.stopped = Some(reason);
                report.tables_not_reached = total - index;
                return Ok(report);
            }
        }
    }
    Ok(report)
}

impl<S: SearchSource> Run<'_, S> {
    fn stop_before(&self) -> Option<StopReason> {
        if self.token.is_cancelled() {
            return Some(StopReason::Cancelled);
        }
        (Instant::now() >= self.deadline).then_some(StopReason::TimedOut)
    }

    fn control(&self) -> OperationControl {
        let remaining: Duration = self.deadline.saturating_duration_since(Instant::now());
        OperationControl::new(
            self.token.child_token(),
            Some(Instant::now() + self.limits.table_timeout.min(remaining)),
        )
    }

    async fn search_one(&self, target: TableTarget) -> Result<Step, PlanError> {
        let columns = match self.source.columns(&target, &self.control()).await {
            Ok(columns) => columns,
            Err(error) => return Ok(self.step_for_error(target, &error)),
        };
        let table = SearchTable { target, columns };
        let Some(plan) = plan_table(self.driver_id, &table, self.term, self.options)? else {
            return Ok(Step::Done(outcome(table.target, TableStatus::NothingToSearch)));
        };
        let result = self.source.query(&plan.statement, &plan.params, &self.control()).await;
        Ok(match result {
            Ok(rows) => Step::Done(read_hits(&plan, rows)),
            Err(error) => self.step_for_error(plan.target, &error),
        })
    }

    fn step_for_error(&self, target: TableTarget, error: &DriverError) -> Step {
        match error {
            DriverError::Cancelled => Step::Stop(StopReason::Cancelled),
            _ if self.token.is_cancelled() => Step::Stop(StopReason::Cancelled),
            DriverError::PolicyDenied(message) => Step::Stop(StopReason::Denied(message.clone())),
            DriverError::TimedOut if Instant::now() >= self.deadline => Step::Stop(StopReason::TimedOut),
            DriverError::TimedOut => Step::Done(outcome(
                target,
                TableStatus::Failed(crate::tr!("The search of this table timed out.")),
            )),
            other => Step::Done(outcome(target, TableStatus::Failed(other.to_string()))),
        }
    }
}

fn outcome(target: TableTarget, status: TableStatus) -> TableOutcome {
    TableOutcome {
        target,
        status,
        hits: Vec::new(),
        matched_columns: Vec::new(),
        more_rows_exist: false,
        columns_not_searched: 0,
    }
}

fn read_hits(plan: &TablePlan, result: QueryResult) -> TableOutcome {
    let shown_count = plan.display.len();
    let limit = usize::try_from(MAX_HITS_PER_TABLE).unwrap_or(usize::MAX);
    let more_rows_exist = result.rows.len() > limit;
    let mut matched_columns: Vec<String> = Vec::new();
    let hits: Vec<TableHit> = result
        .rows
        .iter()
        .take(limit)
        .map(|row| {
            let matched: Vec<String> = plan
                .searched
                .iter()
                .zip(row.iter().skip(shown_count))
                .filter(|(_, flag)| flag_is_set(flag))
                .map(|(name, _)| name.clone())
                .collect();
            for name in &matched {
                if !matched_columns.contains(name) {
                    matched_columns.push(name.clone());
                }
            }
            let shown = plan.display.iter().cloned().zip(row.iter().cloned()).collect();
            TableHit { shown, matched }
        })
        .collect();
    TableOutcome {
        target: plan.target.clone(),
        status: TableStatus::Searched,
        hits,
        matched_columns,
        more_rows_exist,
        columns_not_searched: plan.columns_not_searched,
    }
}

fn flag_is_set(value: &Value) -> bool {
    match value {
        Value::Int(number) => *number != 0,
        Value::Bool(flag) => *flag,
        Value::Float(number) => *number != 0.0,
        Value::Decimal(number) => !number.is_zero(),
        Value::Text(text) => text.trim() == "1",
        _ => false,
    }
}
