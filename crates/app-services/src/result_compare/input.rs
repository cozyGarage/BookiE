use std::sync::Arc;

use bookie_core::{ColumnInfo, QueryResult, Value};
use tokio_util::sync::CancellationToken;

use super::{CompareError, MAX_COMPARE_ROWS};
use crate::result_scan::{ScanProgress, ScanSource, scan_pages};

pub enum CompareInput {
    Pages(Vec<Arc<QueryResult>>),
    Spool(ScanSource),
}

impl CompareInput {
    pub fn columns(&self) -> &[ColumnInfo] {
        match self {
            Self::Pages(pages) => pages.first().map(|page| page.columns.as_slice()).unwrap_or_default(),
            Self::Spool(source) => source.columns(),
        }
    }

    pub(crate) fn for_each_row(
        &self,
        cancel: &CancellationToken,
        progress: &ScanProgress,
        mut visit: impl FnMut(&[Value]) -> Result<(), CompareError>,
    ) -> Result<u64, CompareError> {
        let mut rows = 0_u64;
        let mut each = |page: &QueryResult| -> Result<(), CompareError> {
            for row in page.rows.iter() {
                rows += 1;
                if rows > MAX_COMPARE_ROWS {
                    return Err(CompareError::TooManyRows);
                }
                visit(row)?;
            }
            Ok(())
        };
        match self {
            Self::Pages(pages) => visit_pages(pages, cancel, progress, &mut each)?,
            Self::Spool(source) => visit_spool(source, cancel, progress, &mut each)?,
        }
        Ok(rows)
    }
}

fn visit_pages(
    pages: &[Arc<QueryResult>],
    cancel: &CancellationToken,
    progress: &ScanProgress,
    each: &mut impl FnMut(&QueryResult) -> Result<(), CompareError>,
) -> Result<(), CompareError> {
    for page in pages {
        if cancel.is_cancelled() {
            return Err(CompareError::Cancelled);
        }
        if page.truncated {
            return Err(CompareError::IncompleteInput);
        }
        each(page)?;
        progress.advance(page.rows.len());
    }
    Ok(())
}

fn visit_spool(
    source: &ScanSource,
    cancel: &CancellationToken,
    progress: &ScanProgress,
    each: &mut impl FnMut(&QueryResult) -> Result<(), CompareError>,
) -> Result<(), CompareError> {
    let mut failure = None;
    let result = scan_pages(source, cancel, progress, |_, page| {
        if page.truncated {
            failure = Some(CompareError::IncompleteInput);
            return Ok(std::ops::ControlFlow::Break(()));
        }
        match each(page) {
            Ok(()) => Ok(std::ops::ControlFlow::Continue(())),
            Err(error) => {
                failure = Some(error);
                Ok(std::ops::ControlFlow::Break(()))
            }
        }
    });
    match failure {
        Some(error) => Err(error),
        None => result.map(|_| ()).map_err(CompareError::from),
    }
}
