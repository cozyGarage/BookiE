use std::sync::Arc;

use bookie_app_services::stream_result_spool::StreamResultSpool;
use bookie_core::ColumnInfo;
use bookie_core::export::ExportError;

use super::{PageFetcher, PageFetcherFactory};
use crate::services::export_pages::PageRunner;

pub(crate) fn spool_export_factory(
    spool: StreamResultSpool,
    columns: Vec<ColumnInfo>,
    expected_rows: usize,
) -> PageFetcherFactory {
    let runner: PageRunner = Arc::new(|future| {
        let (send, receive) = async_channel::bounded(1);
        relm4::spawn(async move {
            let _ = send.send(future.await).await;
        });
        receive
            .recv_blocking()
            .unwrap_or_else(|_| Err(ExportError::Source("the export was interrupted".into())))
    });
    spool_export_factory_with_runner(spool, columns, expected_rows, runner)
}

fn spool_export_factory_with_runner(
    spool: StreamResultSpool,
    columns: Vec<ColumnInfo>,
    expected_rows: usize,
    runner: PageRunner,
) -> PageFetcherFactory {
    Arc::new(move |cancel| {
        let spool = spool.clone();
        let columns = columns.clone();
        let runner = runner.clone();
        let page_count = spool.page_count();
        let mut index = 0_usize;
        let mut rows_read = 0_usize;
        PageFetcher::new(
            move || {
                if cancel.is_cancelled() {
                    return Err(ExportError::Cancelled);
                }
                if index == page_count {
                    return if page_count > 0 && rows_read == expected_rows {
                        Ok(None)
                    } else {
                        Err(ExportError::Source("the saved query result is incomplete".into()))
                    };
                }
                let spool = spool.clone();
                let columns = columns.clone();
                let page_index = index as u64;
                let rows = runner(Box::pin(async move {
                    let page = spool
                        .load_page(page_index, columns)
                        .await
                        .map_err(|_| ExportError::Source("a saved result page could not be read".into()))?;
                    let page = Arc::try_unwrap(page).unwrap_or_else(|page| (*page).clone());
                    Ok(page.rows.into_rows())
                }))?;
                if cancel.is_cancelled() {
                    return Err(ExportError::Cancelled);
                }
                rows_read = rows_read
                    .checked_add(rows.len())
                    .ok_or_else(|| ExportError::Source("the saved result row count is too large".into()))?;
                index += 1;
                Ok(Some(rows))
            },
            || Ok(()),
        )
    })
}

#[cfg(test)]
#[path = "spool_tests.rs"]
mod tests;
