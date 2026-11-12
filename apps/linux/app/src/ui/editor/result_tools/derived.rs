use std::sync::atomic::{AtomicU64, Ordering};

use bookie_app_services::result_filter::FilteredResult;
use sha2::{Digest, Sha256};

use super::super::stream_pages::{ResidentPageStore, StreamTerminal, StreamedResult};
use super::super::{StatementIdentity, StatementOutcome, StatementOutcomeKind};
use crate::ui::export_dialog::{StreamExportQuery, StreamExportRequest};

const DERIVED_GENERATION_BASE: u64 = 1 << 62;
const PREVIEW_CHARS: usize = 60;

static NEXT_DERIVED: AtomicU64 = AtomicU64::new(0);

pub(super) fn next_generation() -> u64 {
    DERIVED_GENERATION_BASE + NEXT_DERIVED.fetch_add(1, Ordering::Relaxed)
}

fn identity(source: StatementIdentity, needle: &str, generation: u64) -> StatementIdentity {
    let mut digest = Sha256::new();
    digest.update(source.digest);
    digest.update(generation.to_be_bytes());
    digest.update((needle.len() as u64).to_be_bytes());
    digest.update(needle.as_bytes());
    StatementIdentity {
        statement_index: source.statement_index,
        digest: digest.finalize().into(),
    }
}

fn preview(needle: &str) -> String {
    let flat: String = needle
        .chars()
        .map(|character| if character.is_control() { ' ' } else { character })
        .take(PREVIEW_CHARS)
        .collect();
    format!("FILTER {flat}")
}

fn filtered_name(name: &str) -> String {
    if name.ends_with("-filtered") {
        name.to_string()
    } else {
        format!("{name}-filtered")
    }
}

fn export_request(
    source: &StreamedResult,
    filtered: &FilteredResult,
    columns: &[bookie_core::ColumnInfo],
) -> Option<StreamExportRequest> {
    let query = source.export.as_ref()?.query.clone();
    Some(StreamExportRequest {
        query: StreamExportQuery {
            driver_id: query.driver_id,
            suggested_name: filtered_name(&query.suggested_name),
        },
        columns: columns.to_vec(),
        spool: Some(filtered.spool.clone()),
        total_rows: filtered.total_rows as usize,
    })
}

pub(super) fn derived_result(source: &StreamedResult, filtered: FilteredResult, generation: u64) -> StreamedResult {
    StreamedResult {
        pages: Vec::new(),
        page_indices: Vec::new(),
        page_row_starts: crate::ui::editor::stream_pages::page_row_starts_from_counts(&filtered.page_row_counts),
        retained_rows: 0,
        columns: source.columns.clone(),
        total_pages: filtered.page_row_counts.len() as u64,
        total_rows: filtered.total_rows as usize,
        terminal: StreamTerminal::Complete,
        export: export_request(source, &filtered, &source.columns),
        generation,
        spool: Some(filtered.spool),
        resident_pages: ResidentPageStore::default(),
        page_row_counts: filtered.page_row_counts,
    }
}

pub(in crate::ui::editor) fn derived_outcome(
    source_identity: StatementIdentity,
    source: &StreamedResult,
    filtered: FilteredResult,
    needle: &str,
) -> StatementOutcome {
    let generation = next_generation();
    let label = preview(needle);
    StatementOutcome {
        identity: identity(source_identity, needle, generation),
        sql_preview: label.clone(),
        sql: label,
        elapsed_ms: 0,
        kind: StatementOutcomeKind::Streamed(Box::new(derived_result(source, filtered, generation))),
        pinned: true,
        detail: Default::default(),
    }
}
