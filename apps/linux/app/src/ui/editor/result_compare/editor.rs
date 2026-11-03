use std::sync::Arc;
use std::time::Instant;

use bookie_app_services::result_compare::{
    CompareError, CompareReport, CompareRequest as EngineRequest, compare_results,
};
use bookie_core::QueryResult;
use bookie_policy::{AuditConnectionContext, AuditRecordPhase, AuditTerminalStatus};
use relm4::ComponentSender;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::selection::Candidate;
use super::summary::{differing_rows, failure_message, notes, summary_text};
use super::ticket::CompareTicket;
use crate::services::database_service::DatabaseService;
use crate::ui::editor::{
    SqlEditor, SqlEditorInput, StatementIdentity, StatementOutcome, StatementOutcomeKind, clear_results_box,
    render_outcomes,
};

pub(in crate::ui) struct CompareRequest {
    pub(super) left: Candidate,
    pub(super) right: Candidate,
    pub(super) keys: Vec<String>,
    pub(super) ticket: Arc<CompareTicket>,
}

impl CompareRequest {
    pub(super) fn new(left: Candidate, right: Candidate, keys: Vec<String>) -> Self {
        Self {
            left,
            right,
            keys,
            ticket: Arc::new(CompareTicket::default()),
        }
    }
}

pub(in crate::ui) struct CompareDone {
    request: CompareRequest,
    outcome: Result<CompareReport, CompareFailure>,
}

impl std::fmt::Debug for CompareRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("compare request")
    }
}

impl std::fmt::Debug for CompareDone {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("compare result")
    }
}

pub(super) enum CompareFailure {
    Audit,
    Compare(CompareError),
}

fn terminal_status(outcome: &Result<CompareReport, CompareError>) -> AuditTerminalStatus {
    match outcome {
        Ok(_) => AuditTerminalStatus::Succeeded,
        Err(CompareError::Cancelled) => AuditTerminalStatus::Cancelled,
        Err(_) => AuditTerminalStatus::Failed,
    }
}

pub(super) async fn run_audited(
    database: Arc<DatabaseService>,
    context: AuditConnectionContext,
    request: &CompareRequest,
) -> Result<CompareReport, CompareFailure> {
    let operation = Uuid::new_v4();
    database
        .audit_result_compare(
            operation,
            context.clone(),
            AuditRecordPhase::Intent,
            AuditTerminalStatus::Pending,
            0,
            None,
        )
        .await
        .map_err(|_| CompareFailure::Audit)?;
    let started = Instant::now();
    let engine = EngineRequest {
        left: request.left.input(),
        right: request.right.input(),
        key_columns: request.keys.clone(),
    };
    let (cancel, progress) = (request.ticket.cancel().clone(), request.ticket.progress().clone());
    let outcome = tokio::task::spawn_blocking(move || compare_results(&engine, &cancel, &progress))
        .await
        .unwrap_or(Err(CompareError::Source));
    let differing = outcome
        .as_ref()
        .map(|report| usize::try_from(differing_rows(&report.summary)).unwrap_or(usize::MAX))
        .unwrap_or(0);
    database
        .audit_result_compare(
            operation,
            context,
            AuditRecordPhase::Outcome,
            terminal_status(&outcome),
            differing,
            u64::try_from(started.elapsed().as_millis()).ok(),
        )
        .await
        .map_err(|_| CompareFailure::Audit)?;
    outcome.map_err(CompareFailure::Compare)
}

fn failure_text(failure: &CompareFailure) -> String {
    match failure {
        CompareFailure::Audit => crate::tr!("The comparison needs the audit journal, which is unavailable"),
        CompareFailure::Compare(error) => failure_message(error),
    }
}

fn diff_identity(request: &CompareRequest) -> StatementIdentity {
    let mut digest = Sha256::new();
    digest.update(Uuid::new_v4().as_bytes());
    digest.update(request.left.label.as_bytes());
    digest.update(request.right.label.as_bytes());
    StatementIdentity {
        statement_index: 0,
        digest: digest.finalize().into(),
    }
}

fn diff_outcome(request: &CompareRequest, diff: Arc<QueryResult>) -> StatementOutcome {
    StatementOutcome {
        identity: diff_identity(request),
        sql_preview: format!("COMPARE {} ↔ {}", request.left.label, request.right.label),
        elapsed_ms: 0,
        kind: StatementOutcomeKind::Rows(vec![diff]),
        pinned: true,
    }
}

impl SqlEditor {
    pub(in crate::ui::editor) fn on_open_compare(&mut self, sender: &ComponentSender<Self>) {
        let found = super::selection::candidates(&self.pinned.displayed());
        if found.len() < 2 {
            self.status
                .set_label(&crate::tr!("Pin or run a second result to compare"));
            return;
        }
        super::dialog::present(&self.results_holder, found, sender.input_sender().clone());
    }

    pub(in crate::ui::editor) fn on_compare_results(
        &mut self,
        request: CompareRequest,
        sender: &ComponentSender<Self>,
    ) {
        let Some(context) = self.audit_context() else {
            request
                .ticket
                .finish(crate::tr!("Open a connection before comparing results"));
            return;
        };
        let database = self.database.clone();
        let input = sender.input_sender().clone();
        relm4::spawn(async move {
            let outcome = run_audited(database, context, &request).await;
            let _ = input.send(SqlEditorInput::CompareFinished(Box::new(CompareDone {
                request,
                outcome,
            })));
        });
    }

    fn audit_context(&self) -> Option<AuditConnectionContext> {
        let metadata = self.database.metadata(self.connection_id?)?;
        Some(AuditConnectionContext {
            id: metadata.id,
            name: metadata.name,
            environment: metadata.environment,
            driver_id: metadata.driver_id,
        })
    }

    pub(in crate::ui::editor) fn on_compare_finished(&mut self, done: CompareDone, sender: &ComponentSender<Self>) {
        let CompareDone { request, outcome } = done;
        let message = match outcome {
            Err(failure) => failure_text(&failure),
            Ok(report)
                if report.diff.rows.is_empty() && report.summary.unknown_left + report.summary.unknown_right == 0 =>
            {
                crate::tr!("No differences · {summary}").replace("{summary}", &summary_text(&report.summary))
            }
            Ok(report) => self.show_diff(&request, report, sender),
        };
        self.status.set_label(&message);
        request.ticket.finish(message);
    }

    fn show_diff(&mut self, request: &CompareRequest, report: CompareReport, sender: &ComponentSender<Self>) -> String {
        let mut message = summary_text(&report.summary);
        for note in notes(&report) {
            message = format!("{message} · {note}");
        }
        self.pinned.add_pin(diff_outcome(request, report.diff));
        clear_results_box(&self.results_holder);
        render_outcomes(
            &self.results_holder,
            &self.pinned.displayed(),
            &self.grid_sender,
            self.connection_id,
            self.database.clone(),
            self.result_view_settings.clone(),
            Some(sender),
        );
        self.refresh_pin_button();
        message
    }
}
