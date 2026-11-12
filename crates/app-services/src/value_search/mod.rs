use std::time::Duration;

use bookie_core::{ColumnInfo, SearchMode, Value};

mod open_filter;
mod plan;
mod run;

pub use open_filter::open_filter;
pub use plan::{PlanError, plan_table, validate_term};
pub use run::{ConnectionSource, SearchSource, run_search};

pub const MAX_SEARCH_TABLES: usize = 200;
pub const MAX_SEARCH_COLUMNS_PER_TABLE: usize = 24;
pub const MAX_HITS_PER_TABLE: u64 = 25;
pub const MAX_DISPLAY_COLUMNS: usize = 3;
pub const MAX_TERM_CHARS: usize = 200;
pub const TABLE_TIMEOUT: Duration = Duration::from_secs(15);
pub const RUN_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchLimits {
    pub table_timeout: Duration,
    pub run_timeout: Duration,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            table_timeout: TABLE_TIMEOUT,
            run_timeout: RUN_TIMEOUT,
        }
    }
}

pub struct SearchRequest<'a> {
    pub driver_id: &'a str,
    pub tables: Vec<TableTarget>,
    pub term: &'a str,
    pub options: SearchOptions,
    pub limits: SearchLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOptions {
    pub mode: SearchMode,
    pub include_non_text: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableTarget {
    pub schema: Option<String>,
    pub name: String,
}

impl TableTarget {
    pub fn label(&self) -> String {
        match &self.schema {
            Some(schema) => format!("{schema}.{}", self.name),
            None => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TablePlan {
    pub target: TableTarget,
    pub statement: String,
    pub params: Vec<Value>,
    pub searched: Vec<String>,
    pub display: Vec<String>,
    pub columns_not_searched: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableHit {
    pub shown: Vec<(String, Value)>,
    pub matched: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableStatus {
    Searched,
    NothingToSearch,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableOutcome {
    pub target: TableTarget,
    pub status: TableStatus,
    pub hits: Vec<TableHit>,
    pub matched_columns: Vec<String>,
    pub more_rows_exist: bool,
    pub columns_not_searched: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    TimedOut,
    Denied(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SearchReport {
    pub tables: Vec<TableOutcome>,
    pub tables_over_limit: usize,
    pub tables_not_reached: usize,
    pub stopped: Option<StopReason>,
}

impl SearchReport {
    pub fn with_hits(&self) -> impl Iterator<Item = &TableOutcome> {
        self.tables.iter().filter(|table| !table.hits.is_empty())
    }

    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if let Some(reason) = &self.stopped {
            notes.push(stop_note(reason, self.tables_not_reached));
        }
        if self.tables_over_limit > 0 {
            notes.push(
                crate::tr!("{count} tables were left out because one search covers at most {limit} tables.")
                    .replace("{count}", &self.tables_over_limit.to_string())
                    .replace("{limit}", &MAX_SEARCH_TABLES.to_string()),
            );
        }
        let failed = self
            .tables
            .iter()
            .filter(|table| matches!(table.status, TableStatus::Failed(_)))
            .count();
        if failed > 0 {
            notes.push(crate::tr!("{count} tables could not be searched.").replace("{count}", &failed.to_string()));
        }
        if self.tables.iter().any(|table| table.columns_not_searched > 0) {
            notes.push(
                crate::tr!("Tables with more than {limit} matching columns were searched in the first {limit} only.")
                    .replace("{limit}", &MAX_SEARCH_COLUMNS_PER_TABLE.to_string()),
            );
        }
        notes
    }
}

fn stop_note(reason: &StopReason, not_reached: usize) -> String {
    let count = not_reached.to_string();
    match reason {
        StopReason::Cancelled => {
            crate::tr!("The search was cancelled. {count} tables were not searched.").replace("{count}", &count)
        }
        StopReason::TimedOut => {
            crate::tr!("The search ran out of time. {count} tables were not searched.").replace("{count}", &count)
        }
        StopReason::Denied(message) => {
            crate::tr!("The search was stopped by policy: {message}. {count} tables were not searched.")
                .replace("{message}", message)
                .replace("{count}", &count)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchProgress {
    pub searched: usize,
    pub total: usize,
    pub current: String,
}

pub struct SearchTable {
    pub target: TableTarget,
    pub columns: Vec<ColumnInfo>,
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod plan_tests;

#[cfg(test)]
#[path = "run_tests.rs"]
mod run_tests;
