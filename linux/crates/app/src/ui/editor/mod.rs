mod completion;
mod diagnostics;
pub(crate) mod open_file;
use tablepro_core::sql_format as format_plan;
mod outcomes;
mod schema;
mod session_mode;
mod sql_text;
mod statement_cursor;

use std::time::SystemTime;

use relm4::adw::prelude::*;
use relm4::gtk::glib;
use relm4::prelude::*;
use relm4::{adw, gtk};
use sourceview5::prelude::*;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use tablepro_core::QueryResult;
use tablepro_storage::query_history::{HistoryStore, NewEntry, Outcome};

use crate::services::database_service::ConnectionMetadata;
use crate::ui::grid::GridMsg;

pub use completion::{SchemaIndex, SchemaRequest, candidate_words, referenced_tables, table_key};
pub use schema::{SQL_KEYWORDS, build_schema_buffer, derive_tab_label, update_schema_buffer};

use outcomes::{ScriptRunResult, clear_box, render_outcomes, run_statements, summary_label};
use schema::{apply_editor_font_size, apply_editor_scheme};
use sql_text::toggle_line_comment;
use statement_cursor::{script_statements, statement_at_cursor};

pub struct SqlEditor {
    catalog_changes: crate::services::catalog::CatalogChanges,
    catalog_origin: Option<crate::services::catalog::CatalogOrigin>,
    diagnostics: diagnostics::Diagnostics,
    source_view: sourceview5::View,
    run_button: gtk::Button,
    session_button: gtk::ToggleButton,
    session: Option<session_mode::EditorSession>,
    session_ending: bool,
    cancel_button: gtk::Button,
    running_spinner: gtk::Spinner,
    results_holder: gtk::Box,
    status: gtk::Label,
    grid_sender: relm4::Sender<GridMsg>,
    cancel_token: Option<CancellationToken>,
    executions: std::collections::HashMap<u64, ExecutionContext>,
    connection_id: Option<Uuid>,
    history: Option<HistoryStore>,
    database: std::sync::Arc<crate::services::database_service::DatabaseService>,
    preferences: crate::services::preferences::PreferencesStore,
    run_generation: RunGeneration,
    drop_generation: std::rc::Rc<DropGeneration>,
    /// Disconnected in `shutdown`. Without this, every tab's
    /// `connect_dark_notify` closure -- which strongly captures this
    /// tab's `source_view` -- stayed registered on the process-global
    /// `AdwStyleManager` forever, keeping the whole tab's widget tree
    /// alive past the tab's own close.
    dark_notify_handler: Option<glib::SignalHandlerId>,
}

pub struct SqlEditorInit {
    pub schema_buffer: gtk::TextBuffer,
    pub schema_index: std::rc::Rc<std::cell::RefCell<SchemaIndex>>,
    pub initial_query: Option<String>,
    pub connection_id: Option<Uuid>,
    pub history: Option<HistoryStore>,
    pub database: std::sync::Arc<crate::services::database_service::DatabaseService>,
    pub preferences: crate::services::preferences::PreferencesStore,
}

#[derive(Debug, Clone)]
pub struct StatementOutcome {
    pub sql_preview: String,
    pub elapsed_ms: u128,
    pub kind: StatementOutcomeKind,
}

#[derive(Debug, Clone)]
pub enum StatementOutcomeKind {
    Rows(QueryResult),
    Error(String),
    NotRun,
}

#[derive(Debug)]
pub enum SqlEditorInput {
    CatalogStatementSucceeded {
        origin: crate::services::catalog::CatalogOrigin,
        driver: String,
        sql: String,
    },
    Run,
    RunWithParameters {
        generation: u64,
        sql: String,
        statements: Vec<String>,
        error_policy: tablepro_core::sql_syntax::script::BatchErrorPolicy,
        values: std::collections::HashMap<String, tablepro_core::Value>,
    },
    Cancel,
    ShowOutcomes {
        generation: u64,
        outcomes: Vec<StatementOutcome>,
    },
    ShowCancelled(u64),
    ShowTimedOut {
        generation: u64,
        secs: u32,
    },
    InsertDroppedSql {
        request: DropRequest,
        text: String,
    },
    DroppedSqlFailed {
        request: DropRequest,
        message: String,
    },
    ReplaceQuery(String),
    Format,
    RunAtCursor,
    ToggleLineComment,
    Explain,
    Grid(GridMsg),
    SessionToggled(bool),
    SessionOpened {
        connection_id: Uuid,
        result: Result<session_mode::OpenedSession, String>,
    },
    SessionState(bool),
    SessionEnd {
        commit: bool,
    },
    SessionCommitFinished(Result<(), String>),
}

#[derive(Debug)]
pub enum SqlEditorOutput {
    CatalogChanged(crate::services::catalog::CatalogOrigin),
    RunStateChanged(bool),
    QueryChanged(String),
    NeedColumns(Vec<String>),
    CopyToClipboard(String),
    ShowRowAsJson(String),
    ExportResults { result: QueryResult, name: String },
}

pub(super) const MAX_SQL_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug)]
struct ExecutionContext {
    sql: String,
    metadata: ConnectionMetadata,
    started_at: SystemTime,
}

#[derive(Debug, Default)]
struct RunGeneration {
    next: u64,
    current: Option<u64>,
    active: std::collections::HashSet<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RunTerminal {
    replace_ui: bool,
    became_idle: bool,
}

impl RunGeneration {
    fn begin(&mut self) -> u64 {
        self.next = self.next.wrapping_add(1);
        self.current = Some(self.next);
        self.next
    }

    fn accepts(&self, generation: u64) -> bool {
        self.current == Some(generation)
    }

    fn start(&mut self, generation: u64) -> bool {
        self.accepts(generation) && self.active.insert(generation)
    }

    fn finish(&mut self, generation: u64) -> Option<RunTerminal> {
        if !self.active.remove(&generation) {
            return None;
        }
        Some(RunTerminal {
            replace_ui: self.accepts(generation),
            became_idle: self.active.is_empty(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DropRequest {
    generation: u64,
    revision: u64,
}

#[derive(Debug, Default)]
struct DropGeneration {
    generation: std::cell::Cell<u64>,
    revision: std::cell::Cell<u64>,
}

impl DropGeneration {
    fn changed(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
    }

    fn begin(&self) -> DropRequest {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        DropRequest {
            generation,
            revision: self.revision.get(),
        }
    }

    fn accepts(&self, request: DropRequest) -> bool {
        self.generation.get() == request.generation && self.revision.get() == request.revision
    }
}

#[relm4::component(pub)]
impl SimpleComponent for SqlEditor {
    type Init = SqlEditorInit;
    type Input = SqlEditorInput;
    type Output = SqlEditorOutput;

    view! {
        adw::ToolbarView {
            add_top_bar = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_margin_top: 8,
                set_margin_bottom: 8,
                set_margin_start: 8,
                set_margin_end: 8,

                gtk::Box {
                    set_hexpand: true,
                },

                #[name = "cursor_info"]
                gtk::Label {
                    set_halign: gtk::Align::End,
                    add_css_class: "dim-label",
                    add_css_class: "monospace",
                    set_margin_end: 8,
                },

                #[name = "warnings_button"]
                gtk::MenuButton {
                    set_label: &crate::tr!("SQL warnings"),
                    set_tooltip_text: Some(crate::tr!("Character suggestions; SQL is not changed").as_str()),
                    set_visible: false,
                },

                #[name = "running_spinner"]
                gtk::Spinner {
                    set_visible: false,
                    set_spinning: true,
                    set_size_request: (20, 20),
                },

                #[name = "status"]
                gtk::Label {
                    set_halign: gtk::Align::End,
                    add_css_class: "dim-label",
                },

                #[name = "cancel_button"]
                gtk::Button {
                    set_label: &crate::tr!("Cancel"),
                    set_tooltip_text: Some(crate::tr!("Cancel running query (Esc)").as_str()),
                    set_visible: false,
                    add_css_class: "flat",
                    connect_clicked => SqlEditorInput::Cancel,
                },

                #[name = "session_button"]
                gtk::ToggleButton {
                    set_label: &crate::tr!("Session"),
                    set_tooltip_text: Some(crate::tr!("Run this tab on its own connection, so settings, temporary tables and transactions carry over between runs").as_str()),
                    add_css_class: "flat",
                    connect_toggled[sender] => move |button| {
                        sender.input(SqlEditorInput::SessionToggled(button.is_active()));
                    },
                },

                #[name = "explain_button"]
                gtk::Button {
                    set_label: &crate::tr!("Explain"),
                    set_tooltip_text: Some(crate::tr!("Explain query plan").as_str()),
                    add_css_class: "flat",
                    connect_clicked => SqlEditorInput::Explain,
                },

                #[name = "run_button"]
                gtk::Button {
                    set_label: &crate::tr!("Run"),
                    set_tooltip_text: Some(crate::tr!("Run query (Ctrl+Return)").as_str()),
                    add_css_class: "suggested-action",
                    connect_clicked => SqlEditorInput::Run,
                },
            },

            #[wrap(Some)]
            set_content = &gtk::Paned {
                set_orientation: gtk::Orientation::Vertical,
                set_position: 280,
                set_vexpand: true,
                set_hexpand: true,

                #[wrap(Some)]
                set_start_child = &gtk::ScrolledWindow {
                    set_min_content_height: 200,

                    #[wrap(Some)]
                    #[name = "source_view"]
                    set_child = &sourceview5::View {
                        set_show_line_numbers: true,
                        set_monospace: true,
                        set_auto_indent: true,
                        set_highlight_current_line: true,
                        set_tab_width: 4,
                        set_top_margin: 8,
                        set_bottom_margin: 8,
                        set_left_margin: 8,
                        set_right_margin: 8,
                    },
                },

                #[wrap(Some)]
                #[name = "results_holder"]
                set_end_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },
            },
        }
    }

    fn init(init: Self::Init, _root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let widgets = view_output!();

        let lang_manager = sourceview5::LanguageManager::default();
        let initial_text = init.initial_query.unwrap_or_else(|| "SELECT 1;".to_string());
        if let Some(lang) = lang_manager.language("sql") {
            let buffer = sourceview5::Buffer::with_language(&lang);
            buffer.set_text(&initial_text);
            widgets.source_view.set_buffer(Some(&buffer));
        } else {
            widgets.source_view.buffer().set_text(&initial_text);
        }
        apply_editor_scheme(&widgets.source_view);
        let view_for_theme = widgets.source_view.clone();
        let dark_notify_handler = adw::StyleManager::default().connect_dark_notify(move |_| {
            apply_editor_scheme(&view_for_theme);
        });

        let font_size = init.preferences.load().editor_font_size;
        apply_editor_font_size(&widgets.source_view, font_size);

        let provider = sourceview5::CompletionWords::new(Some("SQL"));
        provider.register(&init.schema_buffer);
        if let Ok(view_buffer) = widgets.source_view.buffer().downcast::<sourceview5::Buffer>() {
            provider.register(&view_buffer);
        }
        let completion = widgets.source_view.completion();
        completion.add_provider(&provider);

        let cursor_info = widgets.cursor_info.clone();
        let view_for_cursor = widgets.source_view.clone();
        let update_cursor = move || {
            let buffer = view_for_cursor.buffer();
            let mark = buffer.get_insert();
            let iter = buffer.iter_at_mark(&mark);
            let line = iter.line() + 1;
            let col = iter.line_offset() + 1;
            cursor_info.set_label(&format!("Ln {line}, Col {col}"));
        };
        update_cursor();
        widgets
            .source_view
            .buffer()
            .connect_cursor_position_notify(move |_| update_cursor());

        let refresh_completion = build_completion_refresh(
            widgets.source_view.clone(),
            init.schema_buffer.clone(),
            init.schema_index.clone(),
            sender.clone(),
        );
        refresh_completion();
        let refresh_on_cursor = refresh_completion.clone();
        widgets
            .source_view
            .buffer()
            .connect_cursor_position_notify(move |_| refresh_on_cursor());

        let drop_generation = std::rc::Rc::new(DropGeneration::default());
        let view_for_change = widgets.source_view.clone();
        let sender_for_change = sender.clone();
        let refresh_on_change = refresh_completion.clone();
        let drop_generation_for_change = drop_generation.clone();
        widgets.source_view.buffer().connect_changed(move |_| {
            drop_generation_for_change.changed();
            let buffer = view_for_change.buffer();
            let (start, end) = buffer.bounds();
            let text = buffer.text(&start, &end, false).to_string();
            let _ = sender_for_change.output(SqlEditorOutput::QueryChanged(text));
            refresh_on_change();
        });

        let run_shortcut = gtk::Shortcut::builder()
            .trigger(&crate::ui::shortcut::parse("<Primary>Return"))
            .action(&gtk::CallbackAction::new({
                let sender = sender.clone();
                move |_, _| {
                    sender.input(SqlEditorInput::Run);
                    glib::Propagation::Stop
                }
            }))
            .build();
        let cancel_shortcut = gtk::Shortcut::builder()
            .trigger(&crate::ui::shortcut::parse("Escape"))
            .action(&gtk::CallbackAction::new({
                let sender = sender.clone();
                move |_, _| {
                    sender.input(SqlEditorInput::Cancel);
                    glib::Propagation::Stop
                }
            }))
            .build();
        let format_shortcut = gtk::Shortcut::builder()
            .trigger(&crate::ui::shortcut::parse("<Primary><Shift>f"))
            .action(&gtk::CallbackAction::new({
                let sender = sender.clone();
                move |_, _| {
                    sender.input(SqlEditorInput::Format);
                    glib::Propagation::Stop
                }
            }))
            .build();
        let run_at_cursor_shortcut = gtk::Shortcut::builder()
            .trigger(&crate::ui::shortcut::parse("<Primary><Shift>Return"))
            .action(&gtk::CallbackAction::new({
                let sender = sender.clone();
                move |_, _| {
                    sender.input(SqlEditorInput::RunAtCursor);
                    glib::Propagation::Stop
                }
            }))
            .build();
        let toggle_comment_shortcut = gtk::Shortcut::builder()
            .trigger(&crate::ui::shortcut::parse("<Primary>slash"))
            .action(&gtk::CallbackAction::new({
                let sender = sender.clone();
                move |_, _| {
                    sender.input(SqlEditorInput::ToggleLineComment);
                    glib::Propagation::Stop
                }
            }))
            .build();
        let controller = gtk::ShortcutController::new();
        controller.add_shortcut(run_shortcut);
        controller.add_shortcut(cancel_shortcut);
        controller.add_shortcut(format_shortcut);
        controller.add_shortcut(run_at_cursor_shortcut);
        controller.add_shortcut(toggle_comment_shortcut);
        widgets.source_view.add_controller(controller);

        let drop_target = gtk::DropTarget::new(gtk::gio::File::static_type(), gtk::gdk::DragAction::COPY);
        let sender_for_drop = sender.clone();
        let drop_generation_for_drop = drop_generation.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            let Ok(file) = value.get::<gtk::gio::File>() else {
                return false;
            };
            let Some(path) = file.path() else {
                return false;
            };
            let sender = sender_for_drop.clone();
            let request = drop_generation_for_drop.begin();
            std::thread::spawn(move || {
                let message = match read_sql_text(&path, MAX_SQL_FILE_BYTES) {
                    Ok(text) => SqlEditorInput::InsertDroppedSql { request, text },
                    Err(message) => SqlEditorInput::DroppedSqlFailed { request, message },
                };
                sender.input(message);
            });
            true
        });
        widgets.source_view.add_controller(drop_target);

        let (grid_sender, grid_receiver) = relm4::channel::<GridMsg>();
        let grid_input = sender.input_sender().clone();
        relm4::spawn_local(grid_receiver.forward(grid_input, SqlEditorInput::Grid));

        let model = SqlEditor {
            catalog_changes: Default::default(),
            catalog_origin: None,
            database: init.database.clone(),
            preferences: init.preferences.clone(),
            diagnostics: diagnostics::Diagnostics::install(
                &widgets.source_view,
                &widgets.warnings_button,
                init.connection_id,
                init.database.clone(),
            ),
            source_view: widgets.source_view.clone(),
            run_button: widgets.run_button.clone(),
            session_button: widgets.session_button.clone(),
            session: None,
            session_ending: false,
            cancel_button: widgets.cancel_button.clone(),
            running_spinner: widgets.running_spinner.clone(),
            results_holder: widgets.results_holder.clone(),
            status: widgets.status.clone(),
            grid_sender,
            cancel_token: None,
            executions: std::collections::HashMap::new(),
            connection_id: init.connection_id,
            history: init.history,
            run_generation: RunGeneration::default(),
            drop_generation,
            dark_notify_handler: Some(dark_notify_handler),
        };
        ComponentParts { model, widgets }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        self.diagnostics.stop();
        self.end_session();
        if let Some(handler) = self.dark_notify_handler.take() {
            adw::StyleManager::default().disconnect(handler);
        }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            SqlEditorInput::CatalogStatementSucceeded { origin, driver, sql } => {
                if !origin.owns_window(self.connection_id, &self.database) {
                    return;
                }
                if self.catalog_origin.as_ref() != Some(&origin) {
                    self.catalog_changes = Default::default();
                    self.catalog_origin = Some(origin.clone());
                }
                if self.catalog_changes.succeeded(&sql, &driver) {
                    let _ = sender.output(SqlEditorOutput::CatalogChanged(origin));
                }
            }
            SqlEditorInput::Grid(GridMsg::CopyToClipboard(text)) => {
                let _ = sender.output(SqlEditorOutput::CopyToClipboard(text));
            }
            SqlEditorInput::Grid(GridMsg::ShowRowAsJson(text)) => {
                let _ = sender.output(SqlEditorOutput::ShowRowAsJson(text));
            }
            SqlEditorInput::Grid(GridMsg::ExportResults(result)) => {
                let _ = sender.output(SqlEditorOutput::ExportResults {
                    result,
                    name: self.export_name(),
                });
            }
            SqlEditorInput::Grid(_) => {}
            SqlEditorInput::SessionToggled(enabled) => self.on_session_toggled(enabled, &sender),
            SqlEditorInput::SessionOpened { connection_id, result } => self.on_session_opened(connection_id, result),
            SqlEditorInput::SessionState(open) => self.on_session_state(open),
            SqlEditorInput::SessionEnd { commit: true } => self.commit_and_end(&sender),
            SqlEditorInput::SessionEnd { commit: false } => self.end_session(),
            SqlEditorInput::SessionCommitFinished(result) => self.on_session_commit_finished(result),
            SqlEditorInput::Run => {
                let buffer = self.source_view.buffer();
                let (start, end) = buffer.bounds();
                let sql = buffer.text(&start, &end, false).to_string();
                let trimmed = sql.trim().to_string();
                if trimmed.is_empty() {
                    self.status.set_label(&crate::tr!("empty query"));
                    return;
                }
                self.begin_run(trimmed, false, sender);
            }

            SqlEditorInput::RunWithParameters {
                generation,
                sql,
                statements,
                error_policy,
                values,
            } => {
                self.execute_sql(generation, sql, statements, error_policy, values, sender);
            }

            SqlEditorInput::ToggleLineComment => {
                toggle_line_comment(&self.source_view.buffer());
            }

            SqlEditorInput::Explain => {
                let buffer = self.source_view.buffer();
                let text = if let Some((a, b)) = buffer.selection_bounds() {
                    buffer.text(&a, &b, true).to_string()
                } else {
                    let (start, end) = buffer.bounds();
                    buffer.text(&start, &end, true).to_string()
                };
                if let Some(window) = self.source_view.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
                    crate::ui::explain_dialog::present(
                        &window,
                        self.connection_id,
                        &text,
                        &self.database,
                        &self.preferences,
                    );
                }
            }

            SqlEditorInput::RunAtCursor => {
                let buffer = self.source_view.buffer();
                let (start, end) = buffer.bounds();
                let sql = buffer.text(&start, &end, false).to_string();
                let cursor_chars = buffer.iter_at_mark(&buffer.get_insert()).offset() as usize;
                let cursor_byte: usize = sql.chars().take(cursor_chars).map(char::len_utf8).sum();
                let driver_id = self.metadata().map(|metadata| metadata.driver_id).unwrap_or_default();
                let Some(statement) = statement_at_cursor(&sql, &driver_id, cursor_byte) else {
                    self.status.set_label(&crate::tr!("No statement at cursor"));
                    return;
                };
                self.begin_run(statement, true, sender);
            }

            SqlEditorInput::Cancel => {
                if let Some(token) = self.cancel_token.take() {
                    token.cancel();
                }
            }

            SqlEditorInput::ShowOutcomes { generation, outcomes } => {
                let Some((terminal, context)) = self.finish_run(generation, &sender) else {
                    return;
                };
                if terminal.replace_ui {
                    self.cancel_token = None;
                }
                let total_ms: u128 = outcomes.iter().map(|o| o.elapsed_ms).sum();
                let n_total = outcomes.len();
                let n_ok = outcomes
                    .iter()
                    .filter(|o| matches!(o.kind, StatementOutcomeKind::Rows(_)))
                    .count();
                let first_error = outcomes.iter().find_map(|o| match &o.kind {
                    StatementOutcomeKind::Error(msg) => Some(msg.clone()),
                    _ => None,
                });

                let total_rows: i64 = outcomes
                    .iter()
                    .filter_map(|o| match &o.kind {
                        StatementOutcomeKind::Rows(qr) => Some(qr.rows.len() as i64),
                        _ => None,
                    })
                    .sum();
                let history_outcome = match &first_error {
                    Some(msg) => Outcome::Error(msg.clone()),
                    None => Outcome::Success,
                };
                let rows_for_history = if total_rows > 0 { Some(total_rows) } else { None };
                self.record_history(context, total_ms as i64, rows_for_history, history_outcome);
                if !terminal.replace_ui {
                    return;
                }

                self.status
                    .set_label(&summary_label(n_total, n_ok, total_ms, first_error.is_some()));
                clear_box(&self.results_holder);
                render_outcomes(
                    &self.results_holder,
                    &outcomes,
                    &self.grid_sender,
                    self.connection_id,
                    self.database.clone(),
                );
            }

            SqlEditorInput::ShowCancelled(generation) => {
                let Some((terminal, context)) = self.finish_run(generation, &sender) else {
                    return;
                };
                let elapsed = SystemTime::now()
                    .duration_since(context.started_at)
                    .map(|duration| duration.as_millis() as i64)
                    .unwrap_or(0);
                self.record_history(context, elapsed, None, Outcome::Cancelled);
                if !terminal.replace_ui {
                    return;
                }
                self.cancel_token = None;
                self.status.set_label(&crate::tr!("cancelled"));
                clear_box(&self.results_holder);
                let cancelled_page = adw::StatusPage::builder()
                    .title(crate::tr!("Query cancelled"))
                    .description(crate::tr!("The running query was stopped."))
                    .icon_name("process-stop-symbolic")
                    .vexpand(true)
                    .build();
                self.results_holder.append(&cancelled_page);
            }

            SqlEditorInput::ShowTimedOut { generation, secs } => {
                let Some((terminal, context)) = self.finish_run(generation, &sender) else {
                    return;
                };
                let elapsed = SystemTime::now()
                    .duration_since(context.started_at)
                    .map(|duration| duration.as_millis() as i64)
                    .unwrap_or(0);
                let secs_str = secs.to_string();
                let reason =
                    crate::tr!("Query exceeded the {n}s timeout configured in Preferences.").replace("{n}", &secs_str);
                self.record_history(context, elapsed, None, Outcome::Error(reason.clone()));
                if !terminal.replace_ui {
                    return;
                }
                self.cancel_token = None;
                self.status.set_label(&crate::tr!("timed out"));
                clear_box(&self.results_holder);
                let page = adw::StatusPage::builder()
                    .title(crate::tr!("Query timed out"))
                    .description(&reason)
                    .icon_name("dialog-warning-symbolic")
                    .vexpand(true)
                    .build();
                self.results_holder.append(&page);
            }

            SqlEditorInput::InsertDroppedSql { request, text } => {
                if !self.drop_generation.accepts(request) {
                    return;
                }
                let buffer = self.source_view.buffer();
                let (start, end) = buffer.bounds();
                if buffer.text(&start, &end, false).trim().is_empty() {
                    buffer.set_text(&text);
                } else {
                    buffer.insert_at_cursor(&text);
                }
            }

            SqlEditorInput::DroppedSqlFailed { request, message } => {
                if self.drop_generation.accepts(request) {
                    self.status.set_label(&message);
                }
            }

            SqlEditorInput::ReplaceQuery(text) => {
                self.source_view.buffer().set_text(&text);
            }

            SqlEditorInput::Format => {
                let buffer = self.source_view.buffer();
                let (start, end) = buffer.bounds();
                let text = buffer.text(&start, &end, false).to_string();
                if text.trim().is_empty() {
                    return;
                }
                let Some(grammar) = self
                    .metadata()
                    .and_then(|metadata| statement_cursor::grammar_for(&metadata.driver_id))
                else {
                    self.status
                        .set_label(&crate::tr!("Formatting is unavailable for this connection"));
                    return;
                };
                let settings = tablepro_core::sql_syntax::script::LexicalSettings::default_for(grammar);
                let formatted = format_plan::format_script(&text, grammar, settings);
                if formatted == text {
                    return;
                }
                buffer.set_text(&formatted);
            }
        }
    }
}

impl SqlEditor {
    fn export_name(&self) -> String {
        let buffer = self.source_view.buffer();
        let (start, end) = buffer.bounds();
        export_name_for_query(&buffer.text(&start, &end, false))
    }

    fn metadata(&self) -> Option<ConnectionMetadata> {
        self.database.metadata(self.connection_id?)
    }

    fn begin_run(&mut self, sql: String, single_statement: bool, sender: ComponentSender<Self>) {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }
        let generation = self.run_generation.begin();
        let driver_id = self.metadata().map(|metadata| metadata.driver_id).unwrap_or_default();
        let (statements, error_policy) = if single_statement {
            (
                vec![sql.clone()],
                tablepro_core::sql_syntax::script::BatchErrorPolicy::StopScript,
            )
        } else {
            match script_statements(&sql, &driver_id) {
                Ok(planned) => (planned.statements, planned.error_policy),
                Err(message) => {
                    self.status.set_label(&message);
                    return;
                }
            }
        };
        let names = crate::services::query_parameters::statement_names(&sql, &driver_id);
        if names.is_empty() {
            self.execute_sql(
                generation,
                sql,
                statements,
                error_policy,
                std::collections::HashMap::new(),
                sender,
            );
            return;
        }
        let Some(window) = self
            .source_view
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok())
        else {
            self.status
                .set_label(&crate::tr!("Cannot ask for parameter values without a window"));
            return;
        };
        crate::ui::parameters_dialog::present(&window, &names, move |values| {
            sender.input(SqlEditorInput::RunWithParameters {
                generation,
                sql: sql.clone(),
                statements: statements.clone(),
                error_policy,
                values,
            });
        });
    }

    fn execute_sql(
        &mut self,
        generation: u64,
        trimmed: String,
        statements: Vec<String>,
        error_policy: tablepro_core::sql_syntax::script::BatchErrorPolicy,
        parameter_values: std::collections::HashMap<String, tablepro_core::Value>,
        sender: ComponentSender<Self>,
    ) {
        if !self.run_generation.accepts(generation) {
            return;
        }
        if self.session_ending {
            self.status.set_label(&crate::tr!("Waiting for session commit"));
            return;
        }
        let origin = crate::services::catalog::CatalogOrigin::capture(self.connection_id, &self.database);
        let conn = match origin.as_ref().and_then(|origin| origin.connection(&self.database)) {
            Some(c) => c,
            None => {
                self.status.set_label(&crate::tr!("no active connection"));
                return;
            }
        };
        let Some(metadata) = self.metadata() else {
            self.status.set_label(&crate::tr!("no active connection"));
            return;
        };
        if !self.run_generation.start(generation) {
            return;
        };

        if let Some(prev) = self.cancel_token.take() {
            prev.cancel();
        }
        let token = CancellationToken::new();
        self.cancel_token = Some(token.clone());

        self.set_running(true, &sender);
        self.status.set_label(&crate::tr!("Running…"));
        clear_box(&self.results_holder);

        self.executions.insert(
            generation,
            ExecutionContext {
                sql: trimmed.clone(),
                metadata,
                started_at: SystemTime::now(),
            },
        );

        let timeout_secs = crate::services::operation_control::configured_timeout_secs(&self.preferences);
        let driver_id = self
            .executions
            .get(&generation)
            .map(|context| context.metadata.driver_id.clone())
            .unwrap_or_default();
        let target = self.statement_target(conn);
        let sender_clone = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    let succeeded = |sql: &str| {
                        if let Some(origin) = &origin {
                            sender_clone.input(SqlEditorInput::CatalogStatementSucceeded {
                                origin: origin.clone(),
                                driver: driver_id.clone(),
                                sql: sql.into(),
                            });
                        }
                    };
                    let msg = match run_statements(
                        target.clone(),
                        statements,
                        &driver_id,
                        &parameter_values,
                        &control,
                        error_policy,
                        succeeded,
                    )
                    .await
                    {
                        ScriptRunResult::Cancelled => SqlEditorInput::ShowCancelled(generation),
                        ScriptRunResult::TimedOut => SqlEditorInput::ShowTimedOut {
                            generation,
                            secs: timeout_secs,
                        },
                        ScriptRunResult::Completed(outcomes) => {
                            let total_ms: u128 = outcomes.iter().map(|o| o.elapsed_ms).sum();
                            let n_ok = outcomes
                                .iter()
                                .filter(|o| matches!(o.kind, StatementOutcomeKind::Rows(_)))
                                .count();
                            let n_err = outcomes
                                .iter()
                                .filter(|o| matches!(o.kind, StatementOutcomeKind::Error(_)))
                                .count();
                            tracing::info!(n_ok, n_err, total_ms, "script run complete");
                            SqlEditorInput::ShowOutcomes { generation, outcomes }
                        }
                    };
                    if let Some(open) = target.transaction_open().await {
                        sender_clone.input(SqlEditorInput::SessionState(open));
                    }
                    sender_clone.input(msg);
                })
                .drop_on_shutdown()
        });
    }

    fn set_running(&self, running: bool, sender: &ComponentSender<Self>) {
        self.run_button.set_sensitive(!running);
        self.cancel_button.set_visible(running);
        self.running_spinner.set_visible(running);
        let _ = sender.output(SqlEditorOutput::RunStateChanged(running));
    }

    fn finish_run(
        &mut self,
        generation: u64,
        sender: &ComponentSender<Self>,
    ) -> Option<(RunTerminal, ExecutionContext)> {
        let terminal = self.run_generation.finish(generation)?;
        let context = self.executions.remove(&generation)?;
        if terminal.became_idle {
            self.set_running(false, sender);
        }
        Some((terminal, context))
    }

    fn record_history(
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
            source: tablepro_storage::query_history::Source::Editor,
        };
        relm4::spawn(async move {
            if let Err(e) = history.record(entry).await {
                tracing::warn!(error = %e, "history record failed");
            }
        });
    }
}

pub(super) fn read_sql_text(path: &std::path::Path, max_bytes: u64) -> Result<String, String> {
    tablepro_core::text_file::read_text_file(path, max_bytes)
        .map(|file| file.text)
        .map_err(|error| open_file::file_error_message(&error))
}

fn export_name_for_query(query: &str) -> String {
    if query.trim().is_empty() {
        return crate::tr!("query-results");
    }
    let label = derive_tab_label(query);
    let mut slug = String::new();
    for character in label.chars() {
        if character.is_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        crate::tr!("query-results")
    } else {
        slug.to_string()
    }
}

fn build_completion_refresh(
    view: sourceview5::View,
    schema_buffer: gtk::TextBuffer,
    schema_index: std::rc::Rc<std::cell::RefCell<SchemaIndex>>,
    sender: ComponentSender<SqlEditor>,
) -> std::rc::Rc<dyn Fn()> {
    std::rc::Rc::new(move || {
        let buffer = view.buffer();
        let (start, end) = buffer.bounds();
        let sql = buffer.text(&start, &end, false).to_string();
        let cursor_chars = buffer.iter_at_mark(&buffer.get_insert()).offset() as usize;
        let cursor_byte: usize = sql.chars().take(cursor_chars).map(char::len_utf8).sum();
        let Ok(index) = schema_index.try_borrow() else {
            return;
        };
        let words = candidate_words(&sql, cursor_byte, &index);
        let missing: Vec<String> = referenced_tables(&sql, cursor_byte)
            .into_iter()
            .filter(|table| !index.knows_columns(table))
            .collect();
        drop(index);
        update_schema_buffer(&schema_buffer, &words);
        if !missing.is_empty() {
            let _ = sender.output(SqlEditorOutput::NeedColumns(missing));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{DropGeneration, RunGeneration, export_name_for_query, read_sql_text};
    use std::io::Write;

    #[test]
    fn stale_run_generations_cannot_finish_newer_runs() {
        let mut generations = RunGeneration::default();
        let first = generations.begin();
        assert!(generations.start(first));
        let second = generations.begin();
        assert!(generations.start(second));

        let first_terminal = generations.finish(first).unwrap();
        assert!(!first_terminal.replace_ui);
        assert!(!first_terminal.became_idle);
        assert!(generations.accepts(second));
        let second_terminal = generations.finish(second).unwrap();
        assert!(second_terminal.replace_ui);
        assert!(second_terminal.became_idle);
    }

    #[test]
    fn newer_run_can_finish_ui_without_reporting_idle_before_superseded_run() {
        let mut generations = RunGeneration::default();
        let first = generations.begin();
        assert!(generations.start(first));
        let second = generations.begin();
        assert!(generations.start(second));

        let second_terminal = generations.finish(second).unwrap();
        assert!(second_terminal.replace_ui);
        assert!(!second_terminal.became_idle);
        let first_terminal = generations.finish(first).unwrap();
        assert!(!first_terminal.replace_ui);
        assert!(first_terminal.became_idle);
    }

    #[test]
    fn export_name_is_stable_and_safe_for_files() {
        assert_eq!(
            export_name_for_query("SELECT * FROM sales.order_items"),
            "select-from-sales-order-item"
        );
        assert_eq!(export_name_for_query("  \n\t"), "query-results");
    }

    #[test]
    fn dropped_sql_completion_requires_latest_drop_and_unchanged_editor() {
        let generations = DropGeneration::default();
        let first = generations.begin();
        let second = generations.begin();
        assert!(!generations.accepts(first));
        assert!(generations.accepts(second));

        generations.changed();
        assert!(!generations.accepts(second));
    }

    #[test]
    fn dropped_sql_reader_enforces_the_exact_byte_limit() {
        let path = std::env::temp_dir().join(format!("tablepro-drop-{}.sql", uuid::Uuid::new_v4()));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"SELECT 1;").unwrap();
        drop(file);

        assert_eq!(read_sql_text(&path, 9).unwrap(), "SELECT 1;");
        assert!(read_sql_text(&path, 8).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
