use std::borrow::Cow;
use std::sync::Arc;

use bookie_app_services::result_chart::{self, ChartData, Profile, Settings};
use bookie_core::QueryResultBudget;
use tokio_util::sync::CancellationToken;

use crate::core_bridge::column_infos_for;
use crate::model::Cell;
use crate::source::ResultSource;

pub const MAX_CHART_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum ChartError {
    Cancelled,
    UnavailableRow,
}

impl ChartError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Cancelled => "Chart preparation was cancelled; choose Chart to try again",
            Self::UnavailableRow => "A result row is no longer available; reload the result before charting it",
        }
    }
}

pub struct ChartState {
    pub settings: Settings,
    pub profiles: Vec<Profile>,
    pub columns: Vec<String>,
    pub rows: Arc<Vec<Vec<Cell>>>,
    pub data: Arc<ChartData>,
    pub total: u64,
    pub incomplete: bool,
}

impl ChartState {
    pub fn load(source: &dyn ResultSource, cancel: &CancellationToken) -> Result<Self, ChartError> {
        let mut budget = QueryResultBudget::with_limits(result_chart::MAX_ROWS, MAX_CHART_BYTES);
        let total = source.total();
        let mut rows = Vec::new();
        for index in 0..total.min(result_chart::MAX_ROWS as u64) {
            if cancel.is_cancelled() {
                return Err(ChartError::Cancelled);
            }
            let row = match source.row_ref(index) {
                Some(row) => Cow::Borrowed(row),
                None => Cow::Owned(source.row(index)),
            };
            if row.len() != source.columns().len() {
                return Err(ChartError::UnavailableRow);
            }
            if !budget.admit(&row) {
                break;
            }
            rows.push(row.into_owned());
        }
        if cancel.is_cancelled() {
            return Err(ChartError::Cancelled);
        }
        let mut columns = column_infos_for(source.columns());
        for (column, meta) in columns.iter_mut().zip(source.columns()) {
            column.data_type.clone_from(&meta.data_type);
            column.primary_key = meta.primary_key;
        }
        let profiles = result_chart::profile(&columns, &rows);
        let settings = result_chart::defaults(&profiles);
        let mut data = result_chart::compute(&rows, &profiles, &settings);
        data.rows_cut = (rows.len() as u64) < total;
        if cancel.is_cancelled() {
            return Err(ChartError::Cancelled);
        }
        Ok(Self {
            settings,
            profiles,
            columns: columns.into_iter().map(|column| column.name).collect(),
            rows: Arc::new(rows),
            data: Arc::new(data),
            total,
            incomplete: source.is_partial(),
        })
    }
}

#[cfg(test)]
#[path = "chart_source_tests.rs"]
mod tests;
