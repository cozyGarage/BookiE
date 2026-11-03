use std::sync::Arc;

use bookie_app_services::result_compare::{CompareError, CompareInput, shared_column_names};
use bookie_app_services::result_scan::ScanSource;
use bookie_core::{ColumnInfo, QueryResult};

use super::super::stream_pages::StreamTerminal;
use super::super::{StatementOutcome, StatementOutcomeKind};

const LABEL_CHARS: usize = 48;

#[derive(Clone)]
enum CandidateSource {
    Pages(Vec<Arc<QueryResult>>),
    Spool(ScanSource),
}

#[derive(Clone)]
pub(super) struct Candidate {
    pub(super) label: String,
    pub(super) columns: Vec<ColumnInfo>,
    source: CandidateSource,
}

impl Candidate {
    pub(super) fn input(&self) -> CompareInput {
        match &self.source {
            CandidateSource::Pages(pages) => CompareInput::Pages(pages.clone()),
            CandidateSource::Spool(source) => CompareInput::Spool(source.clone()),
        }
    }
}

fn short(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|character| if character.is_control() { ' ' } else { character })
        .take(LABEL_CHARS)
        .collect();
    flat.trim().to_string()
}

fn label(preview: &str, rows: u64, set: Option<usize>) -> String {
    let rows = crate::trn!("{n} row", "{n} rows", rows as usize)
        .replace("{n}", &crate::ui::count_format::format_thousands(u128::from(rows)));
    match set {
        Some(set) => format!("{} · #{} · {rows}", short(preview), set + 1),
        None => format!("{} · {rows}", short(preview)),
    }
}

pub(super) fn candidates(outcomes: &[StatementOutcome]) -> Vec<Candidate> {
    let mut found = Vec::new();
    for outcome in outcomes {
        match &outcome.kind {
            StatementOutcomeKind::Rows(sets) => {
                for (index, set) in sets.iter().enumerate() {
                    if set.truncated || set.columns.is_empty() {
                        continue;
                    }
                    let rows = set.rows.len() as u64;
                    found.push(Candidate {
                        label: label(&outcome.sql_preview, rows, (sets.len() > 1).then_some(index)),
                        columns: set.columns.clone(),
                        source: CandidateSource::Pages(vec![set.clone()]),
                    });
                }
            }
            StatementOutcomeKind::Streamed(result) if matches!(result.terminal, StreamTerminal::Complete) => {
                let Some(spool) = result.spool.clone() else { continue };
                let rows = result.total_rows as u64;
                found.push(Candidate {
                    label: label(&outcome.sql_preview, rows, None),
                    columns: result.columns.clone(),
                    source: CandidateSource::Spool(ScanSource::new(spool, result.columns.clone(), rows)),
                });
            }
            _ => {}
        }
    }
    found
}

pub(super) fn key_choices(left: &Candidate, right: &Candidate) -> Result<Vec<String>, CompareError> {
    shared_column_names(&left.columns, &right.columns)
}

pub(in crate::ui::editor) fn can_compare(outcomes: &[StatementOutcome]) -> bool {
    candidates(outcomes).len() >= 2
}
