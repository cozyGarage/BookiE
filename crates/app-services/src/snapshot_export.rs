use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bookie_core::export::{ExportError, RowPage};
use bookie_core::{DriverError, OperationControl, QueryCursorId, Session};
use bookie_policy::PolicyGuard;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::browse_query::{BoundQuery, BrowseTarget};
use crate::export_pages::{PageFuture, PagePlan, PageRunner};
use crate::operation_control;

pub const SNAPSHOT_BEGIN: &str = "BEGIN READ ONLY ISOLATION LEVEL REPEATABLE READ";

struct Held {
    session: Box<dyn Session>,
    cursor: Option<QueryCursorId>,
}

type Shared = Arc<Mutex<Option<Held>>>;

pub struct SnapshotPages {
    guard: Arc<PolicyGuard>,
    plan: PagePlan,
    run: PageRunner,
    cancel: CancellationToken,
    held: Shared,
    complete: Arc<AtomicBool>,
    started: bool,
    finished: bool,
}

impl SnapshotPages {
    pub fn new(guard: Arc<PolicyGuard>, plan: PagePlan, run: PageRunner, cancel: CancellationToken) -> Self {
        Self {
            guard,
            plan,
            run,
            cancel,
            held: Arc::new(Mutex::new(None)),
            complete: Arc::new(AtomicBool::new(false)),
            started: false,
            finished: false,
        }
    }

    pub fn next_page(&mut self) -> Result<Option<RowPage>, ExportError> {
        if self.finished {
            return Ok(None);
        }
        let control = OperationControl::new(
            self.cancel.clone(),
            operation_control::deadline_for(self.plan.timeout_secs),
        );
        let held = self.held.clone();
        let complete = self.complete.clone();
        let future: PageFuture = if self.started {
            Box::pin(async move { fetch(&held, &complete, &control).await })
        } else {
            let guard = self.guard.clone();
            let plan = self.plan.clone();
            Box::pin(async move { start(&guard, &plan, &held, &complete, &control).await })
        };
        self.started = true;
        let rows = (self.run)(future)?;
        self.finished = self.complete.load(Ordering::SeqCst) || rows.is_empty();
        Ok((!rows.is_empty()).then_some(rows))
    }

    pub fn finish(self) -> Result<(), ExportError> {
        let held = self.held;
        (self.run)(Box::pin(async move { close(&held).await })).map(drop)
    }
}

fn map_error(error: DriverError) -> ExportError {
    match error {
        DriverError::Cancelled => ExportError::Cancelled,
        other => ExportError::Source(other.to_string()),
    }
}

fn snapshot_query(plan: &PagePlan) -> Result<BoundQuery, ExportError> {
    BrowseTarget {
        driver_id: &plan.driver_id,
        schema: plan.schema.as_deref(),
        table: &plan.table,
        columns: &plan.columns,
        filter: &plan.filter,
        hidden_columns: None,
        long_text_preview: false,
    }
    .full_select(plan.sort, plan.row_id_order)
    .map_err(ExportError::Source)
}

async fn start(
    guard: &PolicyGuard,
    plan: &PagePlan,
    held: &Shared,
    complete: &AtomicBool,
    control: &OperationControl,
) -> Result<RowPage, ExportError> {
    let query = snapshot_query(plan)?;
    let session = guard.open_session_with_audit().await.map_err(map_error)?;
    let mut slot = held.lock().await;
    let held_session = slot.insert(Held {
        session: Box::new(session),
        cursor: None,
    });
    held_session
        .session
        .query_params_controlled(SNAPSHOT_BEGIN, &[], control)
        .await
        .map_err(map_error)?;
    let cursor = held_session
        .session
        .open_query_cursor_controlled(&query.sql, &query.params, control)
        .await
        .map_err(map_error)?;
    held_session.cursor = Some(cursor);
    drop(slot);
    fetch(held, complete, control).await
}

async fn fetch(held: &Shared, complete: &AtomicBool, control: &OperationControl) -> Result<RowPage, ExportError> {
    let mut slot = held.lock().await;
    let held_session = slot
        .as_mut()
        .ok_or_else(|| ExportError::Source("the snapshot session is closed".into()))?;
    let cursor = held_session
        .cursor
        .ok_or_else(|| ExportError::Source("the snapshot cursor is not open".into()))?;
    let page = held_session
        .session
        .fetch_query_cursor_page_controlled(cursor, control)
        .await
        .map_err(map_error)?;
    if page.result.truncated {
        return Err(ExportError::Source("a page of rows exceeded the result limits".into()));
    }
    complete.store(!page.has_next_page, Ordering::SeqCst);
    Ok(page.result.rows.into_rows())
}

async fn close(held: &Shared) -> Result<RowPage, ExportError> {
    let taken = held.lock().await.take();
    if let Some(held_session) = taken {
        held_session.session.close().await.map_err(map_error)?;
    }
    Ok(Vec::new())
}

#[cfg(test)]
#[path = "snapshot_export_tests.rs"]
mod tests;
