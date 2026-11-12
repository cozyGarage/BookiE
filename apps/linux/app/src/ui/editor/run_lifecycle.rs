use super::*;
use relm4::ComponentSender;

const STALE_RESULTS_CLASS: &str = "tp-stale-results";

pub(super) fn show_cancel_button(running: bool, supports_server_cancellation: bool) -> bool {
    running && supports_server_cancellation
}

pub(super) fn dim_previous_results(holder: &gtk::Box) {
    if holder.first_child().is_some() {
        holder.add_css_class(STALE_RESULTS_CLASS);
    }
}

#[cfg(test)]
pub(super) fn clear_results_box(holder: &gtk::Box) {
    holder.remove_css_class(STALE_RESULTS_CLASS);
    super::outcomes::clear_box(holder);
}

impl SqlEditor {
    pub(super) fn export_name(&self) -> String {
        let buffer = self.source_view.buffer();
        let (start, end) = buffer.bounds();
        export_name_for_query(&buffer.text(&start, &end, true))
    }

    pub(super) fn metadata(&self) -> Option<ConnectionMetadata> {
        self.database.metadata(self.connection_id?)
    }

    pub(super) fn begin_run(
        &mut self,
        sql: String,
        anchor: Option<SourceAnchor>,
        single_statement: bool,
        sender: ComponentSender<Self>,
    ) {
        if !self.can_start_run() {
            return;
        }
        if !self.gate_run_for_pending_edits(
            result_editing::DeferredRun {
                sql: sql.clone(),
                anchor,
                single_statement,
            },
            &sender,
        ) {
            return;
        }
        self.starts_quiet_run();
        self.forget_result_edits();
        self.dismiss_completion_popups();
        if single_statement {
            let planned = PlannedStatements {
                statements: vec![sql.clone()],
                source_ranges: std::iter::once(0..sql.len()).collect(),
                error_policy: bookie_core::sql_syntax::script::BatchErrorPolicy::StopScript,
                settings_check: None,
            };
            self.run_planned(sql, anchor, planned, sender);
        } else {
            self.read_lexing_then_run(sql, anchor, sender);
        }
    }

    pub(super) fn plan_and_run(&mut self, sql: String, anchor: Option<SourceAnchor>, sender: ComponentSender<Self>) {
        if !self.can_start_run() {
            return;
        }
        self.dismiss_completion_popups();
        let driver_id = self.metadata().map(|metadata| metadata.driver_id).unwrap_or_default();
        match script_statements(&sql, self.lexing(&driver_id)) {
            Ok(planned) => self.run_planned(sql, anchor, planned, sender),
            Err(message) => self.status.set_label(&message),
        }
    }

    fn dismiss_completion_popups(&self) {
        completion_provider::dismiss(&self.source_view);
        completion_provider::dismiss(self.split_view.secondary_view());
    }

    fn run_planned(
        &mut self,
        sql: String,
        anchor: Option<SourceAnchor>,
        planned: PlannedStatements,
        sender: ComponentSender<Self>,
    ) {
        let driver_id = self.metadata().map(|metadata| metadata.driver_id).unwrap_or_default();
        let names = crate::services::query_parameters::statement_names(&sql, &driver_id);
        if names.is_empty() {
            self.execute_sql(sql, anchor, planned, std::collections::HashMap::new(), sender);
            return;
        }
        let Some(window) = self
            .source_view
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok())
        else {
            self.status.set_label(&bookie_app_services::tr!(
                "Cannot ask for parameter values without a window"
            ));
            return;
        };
        crate::ui::parameters_dialog::present(&window, &names, move |values| {
            sender.input(SqlEditorInput::RunWithParameters {
                sql: sql.clone(),
                anchor,
                planned: planned.clone(),
                values,
            });
        });
    }

    pub(super) fn offer_longer_timeout(&mut self, exceeded_secs: u32, sql: String) {
        let Some(secs) = self.timeout_retry.offer(exceeded_secs, sql) else {
            return;
        };
        self.longer_timeout_button.set_label(&timeout_retry::label(secs));
        self.longer_timeout_button.set_visible(true);
    }

    pub(super) fn set_running(
        &self,
        running: bool,
        supports_server_cancellation: bool,
        sender: &ComponentSender<Self>,
    ) {
        self.run_button.set_sensitive(!running && self.can_start_run());
        self.cancel_button
            .set_visible(show_cancel_button(running, supports_server_cancellation));
        self.running_spinner.set_visible(running);
        let _ = sender.output(SqlEditorOutput::RunStateChanged(running));
    }

    pub(super) fn can_start_run(&self) -> bool {
        !self.session_open_in_flight && !self.session_teardown_active && self.run_generation.active.is_empty()
    }

    pub(super) fn refresh_run_button(&self) {
        self.run_button.set_sensitive(self.can_start_run());
    }

    pub(super) fn finish_run(
        &mut self,
        generation: u64,
        sender: &ComponentSender<Self>,
    ) -> Option<(RunTerminal, ExecutionContext)> {
        let terminal = self.run_generation.finish(generation)?;
        let context = self.executions.remove(&generation)?;
        if terminal.became_idle {
            self.set_running(false, false, sender);
            (self.completion_refresh)();
            if let Ok(elapsed) = context.started_at.elapsed() {
                finish_notice::notify_if_unattended(&self.source_view, elapsed);
            }
            self.try_start_session_teardown(sender);
        }
        Some((terminal, context))
    }

    pub(super) fn on_outcomes(
        &mut self,
        generation: u64,
        mut outcomes: Vec<StatementOutcome>,
        sender: &ComponentSender<Self>,
    ) {
        let Some((terminal, context)) = self.finish_run(generation, sender) else {
            return;
        };
        if terminal.replace_ui {
            self.cancel_token = None;
        }
        error_navigation::locate(generation, &context, &mut outcomes);
        let totals = outcomes::RunTotals::of(&outcomes);
        self.mark_statements(&context, terminal.replace_ui, &outcomes);
        if self.ends_quiet_run() {
            let _ = context;
        } else {
            self.record_history(
                context,
                totals.elapsed_ms as i64,
                totals.history_rows(),
                totals.history_outcome(),
            );
        }
        if !terminal.replace_ui {
            return;
        }

        let (timing, messages) = super::outcome_detail::totals(&outcomes);
        self.status
            .set_label(&bookie_app_services::server_messages_view::status_with_messages(
                &totals.summary(),
                &messages,
            ));
        self.status.set_tooltip_text(Some(&timing.tooltip()));
        let shown = self.pinned.show(outcomes);
        self.refresh_pin_button();
        self.clear_result_surface();
        render_outcomes(
            self.results_surface(),
            &shown,
            &self.grid_sender,
            self.connection_id,
            self.database.clone(),
            self.result_view_settings.clone(),
            Some(sender),
        );
        self.decide_result_edits(&shown, sender);
    }

    pub(super) fn record_history(
        &self,
        context: ExecutionContext,
        duration_ms: i64,
        rows_affected: Option<i64>,
        outcome: Outcome,
    ) {
        let Some(history) = self.history.clone() else {
            return;
        };
        let entry = NewEntry {
            query: context.sql,
            driver_id: context.metadata.driver_id,
            connection_id: context.metadata.id,
            connection_name: context.metadata.name,
            executed_at: context.started_at,
            duration_ms: Some(duration_ms),
            rows_affected,
            outcome,
            source: bookie_storage::query_history::Source::Editor,
        };
        relm4::spawn(async move {
            if let Err(e) = history.record(entry).await {
                tracing::warn!(error = %e, "history record failed");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::gtk::prelude::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn previous_results_are_dimmed_while_a_run_is_in_flight_and_cleared_with_their_replacement() {
        gtk::init().unwrap();
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);

        dim_previous_results(&holder);
        assert!(
            !holder.has_css_class(STALE_RESULTS_CLASS),
            "nothing to dim on a first run"
        );

        holder.append(&gtk::Label::new(Some("previous result")));
        dim_previous_results(&holder);
        assert!(holder.has_css_class(STALE_RESULTS_CLASS));
        assert!(holder.first_child().is_some(), "the previous result stays on screen");

        clear_results_box(&holder);
        assert!(!holder.has_css_class(STALE_RESULTS_CLASS));
        assert!(holder.first_child().is_none());
    }
}
