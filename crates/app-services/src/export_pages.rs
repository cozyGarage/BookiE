use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bookie_core::export::{ExportError, RowPage};
use bookie_core::{ColumnInfo, Connection, DriverError, FilterSet, OperationControl};
use tokio_util::sync::CancellationToken;

use crate::browse_query::{BrowseTarget, PageQuery};
use crate::operation_control;

pub const EXPORT_PAGE_ROWS: u64 = 5_000;

pub type PageFuture = Pin<Box<dyn Future<Output = Result<RowPage, ExportError>> + Send>>;
pub type PageRunner = Arc<dyn Fn(PageFuture) -> Result<RowPage, ExportError> + Send + Sync>;

#[derive(Clone)]
pub struct PagePlan {
    pub driver_id: String,
    pub schema: Option<String>,
    pub table: String,
    pub columns: Vec<ColumnInfo>,
    pub filter: FilterSet,
    pub sort: Option<(usize, bool)>,
    pub row_id_order: bool,
    pub timeout_secs: u32,
}

fn page_future(
    conn: Arc<dyn Connection>,
    plan: &PagePlan,
    offset: u64,
    cancel: &CancellationToken,
) -> Result<PageFuture, ExportError> {
    let target = BrowseTarget {
        driver_id: &plan.driver_id,
        schema: plan.schema.as_deref(),
        table: &plan.table,
        columns: &plan.columns,
        filter: &plan.filter,
        hidden_columns: None,
        long_text_preview: false,
    };
    let page = if plan.row_id_order {
        target.page_by_row_id(offset, EXPORT_PAGE_ROWS, plan.sort, None)
    } else {
        target.page(offset, EXPORT_PAGE_ROWS, plan.sort, None)
    };
    let query = page.map_err(ExportError::Source)?;
    let schema = plan.schema.clone();
    let table = plan.table.clone();
    let deadline = operation_control::deadline_for(plan.timeout_secs);
    let control = OperationControl::new(cancel.clone(), deadline);
    Ok(Box::pin(async move {
        let result = match query {
            PageQuery::Native => {
                conn.fetch_rows_controlled(schema.as_deref(), &table, offset, EXPORT_PAGE_ROWS, &control)
                    .await
            }
            PageQuery::Sql(query) => conn.query_params_controlled(&query.sql, &query.params, &control).await,
        }
        .map_err(|error| match error {
            DriverError::Cancelled => ExportError::Cancelled,
            other => ExportError::Source(other.to_string()),
        })?;
        if result.truncated {
            return Err(ExportError::Source("a page of rows exceeded the result limits".into()));
        }
        Ok(result.rows.into_rows())
    }))
}

pub fn page_fetcher(
    conn: Arc<dyn Connection>,
    plan: PagePlan,
    run: PageRunner,
    cancel: CancellationToken,
) -> impl FnMut() -> Result<Option<RowPage>, ExportError> + Send {
    let mut offset = 0u64;
    let mut finished = false;
    move || {
        if finished {
            return Ok(None);
        }
        let rows = run(page_future(conn.clone(), &plan, offset, &cancel)?)?;
        finished = (rows.len() as u64) < EXPORT_PAGE_ROWS;
        offset += rows.len() as u64;
        Ok((!rows.is_empty()).then_some(rows))
    }
}

#[cfg(test)]
#[path = "export_pages_tests.rs"]
mod tests;
