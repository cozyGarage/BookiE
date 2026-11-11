use super::stream_pages::{StreamPageCache, StreamTerminal};
use super::*;
use relm4::ComponentSender;

pub(in crate::ui) enum Input {
    Started(StreamStart),
    Page {
        generation: u64,
        sequence: u64,
        page: std::sync::Arc<QueryResult>,
        spooled: bool,
        accepted: tokio::sync::oneshot::Sender<bool>,
    },
    LoadPage {
        generation: u64,
        index: u64,
        identity: StatementIdentity,
        spool: bookie_app_services::stream_result_spool::StreamResultSpool,
        columns: Vec<bookie_core::ColumnInfo>,
    },
    PageLoaded {
        generation: u64,
        index: u64,
        identity: StatementIdentity,
        result: Result<std::sync::Arc<QueryResult>, String>,
    },
    Finished {
        generation: u64,
        terminal: StreamTerminal,
    },
}

pub(in crate::ui) struct StreamStart {
    pub(super) generation: u64,
    pub(super) identity: StatementIdentity,
    pub(super) sql_preview: String,
    pub(super) sql: String,
    pub(super) export_query: Option<crate::ui::export_dialog::StreamExportQuery>,
    pub(super) spool: bookie_app_services::stream_result_spool::StreamResultSpool,
    pub(super) accepted: tokio::sync::oneshot::Sender<bool>,
}

pub(super) struct ActiveStream {
    generation: u64,
    identity: StatementIdentity,
    sql_preview: String,
    sql: String,
    export_query: Option<crate::ui::export_dialog::StreamExportQuery>,
    spool: bookie_app_services::stream_result_spool::StreamResultSpool,
    cache: StreamPageCache,
    error: Option<String>,
}

impl ActiveStream {
    fn outcome(&mut self, terminal: StreamTerminal) -> StatementOutcome {
        StatementOutcome {
            identity: self.identity,
            sql_preview: self.sql_preview.clone(),
            sql: self.sql.clone(),
            elapsed_ms: 0,
            kind: StatementOutcomeKind::Streamed(Box::new(self.cache.snapshot_with_spool(
                terminal,
                self.export_query.clone(),
                self.generation,
                Some(self.spool.clone()),
            ))),
            pinned: false,
        }
    }
}

fn take_stream_for_generation(streaming: &mut Option<ActiveStream>, generation: u64) -> Option<ActiveStream> {
    if streaming.as_ref().is_some_and(|stream| stream.generation == generation) {
        return streaming.take();
    }
    None
}

impl std::fmt::Debug for Input {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("stream input")
    }
}

impl SqlEditor {
    pub(super) fn on_stream_input(&mut self, input: Input, sender: &ComponentSender<Self>) {
        match input {
            Input::Started(start) => self.on_stream_started(start, sender),
            Input::Page {
                generation,
                sequence,
                page,
                spooled,
                accepted,
            } => self.on_stream_page(generation, sequence, page, spooled, accepted, sender),
            Input::LoadPage {
                generation,
                index,
                identity,
                spool,
                columns,
            } => load_stream_page(sender, generation, index, identity, spool, columns),
            Input::PageLoaded {
                generation,
                index,
                identity,
                result,
            } => super::stream_view::complete_page_load(
                super::stream_view::PageLoadContext {
                    holder: self.results_surface(),
                    generation,
                    index,
                    identity,
                    grid_sender: &self.grid_sender,
                    connection_id: self.connection_id,
                    database: self.database.clone(),
                    settings_store: self.result_view_settings.clone(),
                },
                result,
            ),
            Input::Finished { generation, terminal } => self.on_stream_finished(generation, terminal, sender),
        }
    }

    pub(super) fn on_stream_started(&mut self, start: StreamStart, sender: &ComponentSender<Self>) {
        let StreamStart {
            generation,
            identity,
            sql_preview,
            sql,
            export_query,
            spool,
            accepted,
        } = start;
        if !self.run_generation.accepts(generation) {
            let _ = accepted.send(false);
            return;
        }
        self.streaming = Some(ActiveStream {
            generation,
            identity,
            sql_preview,
            sql,
            export_query,
            spool,
            cache: StreamPageCache::default(),
            error: None,
        });
        self.stream_eviction_cursor = Default::default();
        if self.results_surface().first_child().is_none() {
            let loading = waiting_for_stream_page();
            self.show_pins_above(&loading, sender);
        }
        let _ = accepted.send(true);
    }

    pub(super) fn on_stream_page(
        &mut self,
        generation: u64,
        sequence: u64,
        page: std::sync::Arc<QueryResult>,
        spooled: bool,
        accepted: tokio::sync::oneshot::Sender<bool>,
        sender: &ComponentSender<Self>,
    ) {
        if !self.run_generation.accepts(generation) {
            let _ = accepted.send(false);
            return;
        }
        let (outcome, total_rows) = {
            let Some(stream) = self.streaming.as_mut().filter(|stream| stream.generation == generation) else {
                let _ = accepted.send(false);
                return;
            };
            if let Err(error) = stream.cache.append_with_storage_status(sequence, page, spooled) {
                let message = match error {
                    stream_pages::PageAppendError::OutOfOrder => {
                        bookie_app_services::tr!("A streamed result page arrived out of order.")
                    }
                    stream_pages::PageAppendError::SchemaChanged => {
                        bookie_app_services::tr!("The result columns changed between pages.")
                    }
                    stream_pages::PageAppendError::InvalidRowWidth => {
                        bookie_app_services::tr!("A result page did not match its columns.")
                    }
                };
                stream.error = Some(message);
                let _ = accepted.send(false);
                return;
            }
            if !spooled {
                stream.error = Some(bookie_app_services::tr!(
                    "BookiE could not keep the complete result in temporary storage. The received rows remain available."
                ));
            }
            (stream.outcome(StreamTerminal::Loading), stream.cache.total_rows())
        };
        self.show_stream_page(outcome, total_rows, sender);
        let _ = accepted.send(true);
    }

    fn show_stream_page(&mut self, outcome: StatementOutcome, total_rows: usize, sender: &ComponentSender<Self>) {
        self.pinned.set_latest(outcome);
        self.refresh_pin_button();
        let target = super::stream_view::LiveStreamTarget {
            grid_sender: &self.grid_sender,
            connection_id: self.connection_id,
            database: self.database.clone(),
            settings_store: self.result_view_settings.clone(),
        };
        let surface = self.results_surface().clone();
        let updated = if let Some(outcome) = self.pinned.latest().last() {
            match &outcome.kind {
                StatementOutcomeKind::Streamed(result) => super::stream_view::update_live_stream(
                    &surface,
                    outcome,
                    result,
                    &target,
                    &mut self.stream_eviction_cursor,
                ),
                _ => false,
            }
        } else {
            false
        };
        if !updated {
            self.clear_result_surface();
            let shown = self.pinned.displayed();
            render_outcomes(
                self.results_surface(),
                &shown,
                &self.grid_sender,
                self.connection_id,
                self.database.clone(),
                self.result_view_settings.clone(),
                Some(sender),
            );
        }
        self.status.set_label(
            &bookie_app_services::tr!("Received {n} rows")
                .replace("{n}", &crate::ui::count_format::format_thousands(total_rows as u128)),
        );
    }

    pub(super) fn on_stream_finished(
        &mut self,
        generation: u64,
        terminal: StreamTerminal,
        sender: &ComponentSender<Self>,
    ) {
        let Some((run_terminal, context)) = self.finish_run(generation, sender) else {
            return;
        };
        let visible_page = self.visible_stream_page(generation);
        let elapsed = SystemTime::now()
            .duration_since(context.started_at)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0);
        let timed_out = (context.timeout_secs, context.sql.clone());
        let (terminal, rows, mut outcomes) = finish_stream_outcomes(
            take_stream_for_generation(&mut self.streaming, generation),
            terminal,
            elapsed,
        );
        super::error_navigation::locate(generation, &context, &mut outcomes);
        self.mark_statements(&context, run_terminal.replace_ui, &outcomes);
        self.finish_stream_history(context, elapsed, rows, &terminal);
        if !run_terminal.replace_ui {
            return;
        }
        self.cancel_token = None;
        self.apply_stream_status(&terminal, elapsed, timed_out);
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
        self.restore_stream_page(generation, visible_page);
    }

    fn finish_stream_history(
        &mut self,
        context: super::ExecutionContext,
        elapsed: i64,
        rows: Option<i64>,
        terminal: &StreamTerminal,
    ) {
        let history_outcome = match terminal {
            StreamTerminal::Complete => Outcome::Success,
            StreamTerminal::Cancelled => Outcome::Cancelled,
            StreamTerminal::TimedOut => Outcome::Error(bookie_app_services::tr!("Query timed out")),
            StreamTerminal::Failed(message) => Outcome::Error(message.to_string()),
            StreamTerminal::Loading => Outcome::Error(bookie_app_services::tr!("Stream ended before completion")),
        };
        if self.ends_quiet_run() {
            let _ = context;
            return;
        }
        self.record_history(context, elapsed, rows.filter(|rows| *rows > 0), history_outcome);
    }

    fn apply_stream_status(&mut self, terminal: &StreamTerminal, elapsed: i64, timed_out: (u32, String)) {
        match terminal {
            StreamTerminal::Complete => self
                .status
                .set_label(&bookie_app_services::tr!("done in {ms} ms").replace("{ms}", &elapsed.max(0).to_string())),
            StreamTerminal::Cancelled => self.status.set_label(&bookie_app_services::tr!("cancelled")),
            StreamTerminal::TimedOut => {
                self.status.set_label(&bookie_app_services::tr!("timed out"));
                self.offer_longer_timeout(timed_out.0, timed_out.1);
            }
            StreamTerminal::Failed(message) => self.status.set_label(message),
            StreamTerminal::Loading => self.status.set_label(&bookie_app_services::tr!("error")),
        }
    }

    fn visible_stream_page(&self, generation: u64) -> Option<u64> {
        super::stream_view::visible_page_index(self.results_surface(), generation)
    }

    fn restore_stream_page(&self, generation: u64, page_index: Option<u64>) {
        if let Some(page_index) = page_index {
            super::stream_view::restore_page_selection(self.results_surface(), generation, page_index);
        }
    }
}

fn finish_stream_outcomes(
    stream: Option<ActiveStream>,
    terminal: StreamTerminal,
    elapsed: i64,
) -> (StreamTerminal, Option<i64>, Vec<StatementOutcome>) {
    let terminal = stream
        .as_ref()
        .and_then(|stream| stream.error.clone())
        .map_or(terminal, |message| StreamTerminal::Failed(message.into()));
    let rows = stream.as_ref().map(|stream| stream.cache.total_rows() as i64);
    let outcome = stream.map(|mut stream| {
        let mut outcome = stream.outcome(terminal.clone());
        outcome.elapsed_ms = elapsed.max(0) as u128;
        outcome
    });
    (terminal, rows, outcome.into_iter().collect())
}

fn load_stream_page(
    sender: &ComponentSender<SqlEditor>,
    generation: u64,
    index: u64,
    identity: StatementIdentity,
    spool: bookie_app_services::stream_result_spool::StreamResultSpool,
    columns: Vec<bookie_core::ColumnInfo>,
) {
    let loader = sender.clone();
    sender.command(move |_, shutdown| {
        shutdown
            .register(async move {
                let result = spool
                    .load_page(index, columns)
                    .await
                    .map_err(|_| bookie_app_services::tr!("The saved result page could not be read.").to_string());
                loader.input(SqlEditorInput::Stream(Input::PageLoaded {
                    generation,
                    index,
                    identity,
                    result,
                }));
            })
            .drop_on_shutdown()
    });
}

fn waiting_for_stream_page() -> adw::StatusPage {
    adw::StatusPage::builder()
        .title(bookie_app_services::tr!("Waiting for result pages"))
        .icon_name("content-loading-symbolic")
        .vexpand(true)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stale_run_completion_keeps_the_newer_stream() {
        let mut streaming = Some(ActiveStream {
            generation: 2,
            identity: StatementIdentity::default(),
            sql_preview: "SELECT 2".into(),
            sql: "SELECT 2".into(),
            export_query: None,
            spool: bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
                .create()
                .expect("test spool"),
            cache: StreamPageCache::default(),
            error: None,
        });

        assert!(take_stream_for_generation(&mut streaming, 1).is_none());
        assert_eq!(streaming.as_ref().map(|stream| stream.generation), Some(2));
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_stream_waits_with_a_loading_status_page() {
        relm4::adw::init().unwrap();
        let page = waiting_for_stream_page();

        assert_eq!(page.title(), "Waiting for result pages");
        assert_eq!(page.icon_name().as_deref(), Some("content-loading-symbolic"));
    }
}
