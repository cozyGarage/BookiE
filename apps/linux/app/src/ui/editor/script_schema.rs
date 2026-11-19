use relm4::gtk;
use relm4::gtk::prelude::*;

use bookie_app_services::schema_context::{SchemaContextError, schema_context_for_run, schema_context_label};
use bookie_app_services::stream_result_spool::StreamResultSpoolManager;
use bookie_core::sql_syntax::script::BatchErrorPolicy;

use super::outcomes::{ScriptRunResult, StatementRun, run_statements_with_spool_manager};
use super::{SqlEditor, SqlEditorInput, StatementOutcomeKind};

impl SqlEditor {
    pub(super) fn schema_session_pinned(&self) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        Some(session.connection_id) == self.connection_id
            && session
                .connection_identity
                .as_ref()
                .is_some_and(|identity| self.database.identity(session.connection_id).as_ref() == Some(identity))
    }

    pub(super) fn schema_prefix(&self) -> Result<Option<String>, String> {
        let driver_id = self.metadata().map(|metadata| metadata.driver_id).unwrap_or_default();
        let typed = self.schema_entry.text().to_string();
        match schema_context_for_run(&driver_id, &typed, self.schema_session_pinned()) {
            Ok(statement) => Ok(statement),
            Err(SchemaContextError::NeedsSession) => Err(bookie_app_services::tr!(
                "Turn on Session to run in the schema you typed"
            )),
            Err(SchemaContextError::Unsupported(_)) => {
                Err(bookie_app_services::tr!("This connection has no schema to choose"))
            }
            Err(SchemaContextError::Invalid(_)) => Err(bookie_app_services::tr!("That schema name cannot be used")),
        }
    }

    pub(super) fn sync_schema_entry(&self, session_label: &str) -> String {
        let driver_id = self.metadata().map(|metadata| metadata.driver_id).unwrap_or_default();
        let supported = bookie_app_services::schema_context::supports_schema_context(&driver_id);
        let pinned = self.schema_session_pinned();
        self.schema_entry.set_visible(supported);
        self.schema_entry
            .set_sensitive(supported && pinned && self.session_action_idle());
        let tip = if pinned {
            bookie_app_services::tr!("Schema for this session. Leave empty to keep the current schema")
        } else {
            bookie_app_services::tr!("Turn on Session to pin a schema. A pooled run cannot keep one")
        };
        self.schema_entry.set_tooltip_text(Some(&tip));
        self.schema_entry
            .update_property(&[gtk::accessible::Property::Label(&bookie_app_services::tr!("Schema"))]);
        schema_context_label(session_label, &self.schema_entry.text(), supported && pinned)
    }
}

fn prefix_failed(outcomes: &[super::StatementOutcome]) -> bool {
    outcomes.iter().any(|outcome| {
        matches!(
            outcome.kind,
            StatementOutcomeKind::Error(_) | StatementOutcomeKind::NotRun
        )
    })
}

fn prefix_failure_message(outcomes: &[super::StatementOutcome]) -> String {
    outcomes
        .iter()
        .find_map(|outcome| match &outcome.kind {
            StatementOutcomeKind::Error(failure) => Some(failure.as_ref().to_string()),
            StatementOutcomeKind::NotRun => Some(bookie_app_services::tr!("The schema change did not run")),
            _ => None,
        })
        .unwrap_or_else(|| bookie_app_services::tr!("The schema change did not run"))
}

pub(super) fn follow_prefix(
    result: ScriptRunResult,
    generation: u64,
    secs: u32,
    statement: String,
) -> Option<SqlEditorInput> {
    match result {
        ScriptRunResult::Completed(outcomes) if !prefix_failed(&outcomes) => None,
        ScriptRunResult::Completed(outcomes) => Some(SqlEditorInput::SchemaPrefixStopped {
            generation,
            statement,
            message: prefix_failure_message(&outcomes),
        }),
        ScriptRunResult::Cancelled => Some(SqlEditorInput::ShowCancelled(generation)),
        ScriptRunResult::TimedOut => Some(SqlEditorInput::ShowTimedOut { generation, secs }),
        ScriptRunResult::Streamed(terminal) => Some(SqlEditorInput::Stream(super::streaming::Input::Finished {
            generation,
            terminal,
        })),
    }
}

pub(super) async fn run_schema_prefix<S>(
    target: super::session_mode::StatementTarget,
    sql: String,
    driver_id: &str,
    parameter_values: &std::collections::HashMap<String, bookie_core::Value>,
    control: &bookie_core::OperationControl,
    succeeded: S,
) -> ScriptRunResult
where
    S: Fn(&str),
{
    run_statements_with_spool_manager(
        StatementRun {
            target,
            statements: vec![sql],
            driver_id,
            parameter_values,
            control,
            error_policy: BatchErrorPolicy::StopScript,
            settings_check: None,
            succeeded,
        },
        StreamResultSpoolManager::default(),
        |_| async { false },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::editor::{StatementIdentity, StatementOutcome};

    fn outcome(kind: StatementOutcomeKind) -> StatementOutcome {
        StatementOutcome {
            identity: StatementIdentity::default(),
            sql_preview: String::new(),
            sql: String::new(),
            elapsed_ms: 0,
            kind,
            pinned: false,
            detail: Default::default(),
        }
    }

    #[test]
    fn a_failed_schema_statement_stops_the_script_and_a_finished_one_does_not() {
        let failed = follow_prefix(
            ScriptRunResult::Completed(vec![outcome(StatementOutcomeKind::Error("denied".into()))]),
            1,
            30,
            "SET search_path TO \"billing\"".into(),
        );
        assert!(matches!(
            &failed,
            Some(SqlEditorInput::SchemaPrefixStopped { statement, message, .. })
                if statement == "SET search_path TO \"billing\"" && message == "denied"
        ));
        assert!(
            follow_prefix(
                ScriptRunResult::Completed(vec![outcome(StatementOutcomeKind::Rows(Vec::new()))]),
                1,
                30,
                String::new(),
            )
            .is_none()
        );
        assert!(matches!(
            follow_prefix(ScriptRunResult::Cancelled, 4, 30, String::new()),
            Some(SqlEditorInput::ShowCancelled(4))
        ));
    }
}
