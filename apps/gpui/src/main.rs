use std::time::{Duration, Instant};

mod assets;
mod catalog_nav;
mod chrome;
mod detail_panel;
mod grid_edit;
mod grid_table;
mod keys;
mod render;
mod text_input;
mod ui_theme;

use bookie_gpui::copy_format::{self, CopyContext, CopyFormat};
use bookie_gpui::cursor::Cursor;
use bookie_gpui::font_settings;
use bookie_gpui::keymap::Keymap;
use bookie_gpui::messages::{MessageKind, MessageLog};
use bookie_gpui::model::*;
use bookie_gpui::palette::{Action, matches as palette_matches, step as palette_step};
use bookie_gpui::selection::{Selection, copy_tsv};
use bookie_gpui::shell::{Catalog, DEFAULT_RESULT_RATIO, ResultView, Run, Tab, Tabs, next_run, result_ratio_from_drag};
use bookie_gpui::source::{Source, Synthetic};
use bookie_gpui::theme::{Style, Theme};
use gpui::{
    App, Bounds, ClipboardItem, Context, Div, Entity, FocusHandle, Focusable, MouseButton, ScrollHandle,
    ScrollStrategy, Stateful, UniformListScrollHandle, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb,
    size,
};
use gpui_kit::component::input::EditorState;
use text_input::{InputEvent, TextInput};
use tokio_util::sync::CancellationToken;
use ui_theme::ThemeExt;

const ROW_HEIGHT: f32 = 26.0;
pub const GUTTER_WIDTH: f32 = 56.0;

enum Mode {
    All,
    Filtered {
        kept: std::sync::Arc<Vec<u32>>,
        label: String,
    },
}

struct GridView {
    table: gpui::Entity<gpui_kit::component::table::TableState<grid_table::GridDelegate>>,
    _table_events: gpui::Subscription,
    keystroke_hook: Option<gpui::Subscription>,
    source: Source,
    table_source: Source,
    mode: Mode,
    pager: Pager,
    continuous: bool,
    find: Entity<TextInput>,
    filter: Entity<TextInput>,
    filter_visible: bool,
    editor: Entity<TextInput>,
    editing: Option<grid_edit::Editing>,
    edits: grid_edit::Edits,
    table_edits: grid_edit::Edits,
    grid_is_query: bool,
    find_reset: bool,
    cell_cursor: Option<Cursor>,
    viewport_rows: usize,
    palette_input: Entity<TextInput>,
    palette_open: bool,
    detail_open: bool,
    catalog: Catalog,
    live: Option<std::sync::Arc<bookie_gpui::live::LiveSession>>,
    connections: Vec<bookie_app_services::read_session::ConnectionSummary>,
    keymap: Keymap,
    palette_index: usize,
    dark_mode: bool,
    tabs: Tabs,
    tab_scroll: ScrollHandle,
    runs: Vec<Run>,
    run_edits: Vec<grid_edit::Edits>,
    run_sources: Vec<Option<Source>>,
    sql_input: Entity<EditorState>,
    active_run: usize,
    run_scroll: ScrollHandle,
    next_run_number: usize,
    result_view: ResultView,
    messages: MessageLog,
    result_ratio: f32,
    result_open: bool,
    resizing_split: bool,
    font: gpui::SharedString,
    fonts: font_settings::FontSettings,
    font_names: Vec<String>,
    palette_fonts: bool,
    palette_address: Option<String>,
    palette_pattern: Option<String>,
    check_histories: Vec<(String, bookie_app_services::checks::History)>,
    sidebar_open: bool,
    import_menu_open: bool,
    sidebar_width: f32,
    resizing_sidebar: bool,
    connection_open: bool,
    database_open: bool,
    sidebar_search: Entity<TextInput>,
    cursor: MatchCursor,
    find_capped: bool,
    operation_cancel: CancellationToken,
    compare: Option<std::sync::Arc<Diff>>,
    status: String,
    selection: Option<Selection>,
    generation: u64,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    started: Instant,
    first_frame_reported: bool,
    last_frame: Option<Instant>,
    frame_intervals: Vec<Duration>,
    measuring: bool,
    pace: Option<Pace>,
}

struct Pace {
    step: u32,
    steps: u32,
    begun: Instant,
}

fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn default_font(installed: &[String]) -> String {
    let wanted = std::env::var("BOOKIE_PROTOTYPE_FONT").ok();
    wanted
        .iter()
        .map(String::as_str)
        .chain(["Fira Sans", "Fira Code"])
        .find(|name| installed.iter().any(|font| font == name))
        .unwrap_or_default()
        .to_string()
}

fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|value| value.parse().ok())
}

impl Focusable for GridView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl GridView {
    fn new(total: u64, started: Instant, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_source(std::sync::Arc::new(Synthetic::new(total)), started, window, cx)
    }

    fn with_source(source: Source, started: Instant, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let total = source.total();
        let find = cx.new(|cx| TextInput::new("Find", cx));
        let filter = cx.new(|cx| TextInput::new("label contains 7", cx));
        cx.subscribe(&find, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Changed => {
                this.invalidate_background();
                this.cursor = MatchCursor::default();
                this.status.clear();
                cx.notify();
            }
            InputEvent::Submit => this.run_find(true, cx),
            InputEvent::Cancel => this.status.clear(),
        })
        .detach();
        cx.subscribe(&filter, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Changed => {
                this.invalidate_background();
                if this.status == "Filtering…" {
                    this.status.clear();
                }
            }
            InputEvent::Submit => this.apply_filter(cx),
            InputEvent::Cancel => {}
        })
        .detach();
        let editor = cx.new(|cx| TextInput::new("", cx));
        cx.subscribe_in(&editor, window, |this, _, event: &InputEvent, window, cx| {
            this.on_editor_event(event, window, cx);
        })
        .detach();
        let mut font_names = cx.text_system().all_font_names();
        font_names.sort();
        font_names.dedup();
        let sidebar_search = cx.new(|cx| TextInput::new("Search tables", cx));
        let sql_input = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("sql")
                .placeholder("SELECT … (Enter adds a line, Ctrl+Enter runs it)")
        });
        cx.subscribe(
            &sql_input,
            |this, _, event: &gpui_kit::component::input::InputEvent, cx| {
                if matches!(
                    event,
                    gpui_kit::component::input::InputEvent::PressEnter { secondary: true, .. }
                ) {
                    this.run_query(cx);
                }
            },
        )
        .detach();
        cx.subscribe(&sidebar_search, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        let palette_input = cx.new(|cx| TextInput::new("Type a command", cx));
        cx.subscribe_in(
            &palette_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Changed => {
                    this.palette_index = 0;
                    let typed = this.palette_input.read(cx).content().to_string();
                    this.palette_address = bookie_gpui::net_check::parse_address(&typed);
                    this.palette_pattern = this
                        .live
                        .as_ref()
                        .filter(|live| live.driver_id == "redis")
                        .and_then(|_| bookie_gpui::pubsub_view::parse_pattern(&typed));
                    cx.notify();
                }
                InputEvent::Submit => this.run_palette(window, cx),
                InputEvent::Cancel => this.close_palette(window, cx),
            },
        )
        .detach();
        let table = cx.new(|cx| {
            gpui_kit::component::table::TableState::new(grid_table::GridDelegate::default(), window, cx)
                .cell_selectable(true)
                .row_header(false)
        });
        let table_focus = gpui::Focusable::focus_handle(table.read(cx), cx);
        let table_events = cx.subscribe_in(
            &table,
            window,
            |this: &mut Self, _, event: &gpui_kit::component::table::TableEvent, window, cx| match event {
                gpui_kit::component::table::TableEvent::SelectCell(row, column) => {
                    let cursor = bookie_gpui::cursor::Cursor {
                        row: *row,
                        column: *column,
                    };
                    this.cell_cursor = Some(cursor);
                    let names = this.column_names();
                    this.status = cursor.label(&names);
                    cx.notify();
                }
                gpui_kit::component::table::TableEvent::DoubleClickedRow(row) => {
                    this.edit_clicked_row(*row, window, cx);
                }
                _ => {}
            },
        );
        GridView {
            fonts: font_settings::load(),
            table,
            _table_events: table_events,
            keystroke_hook: None,
            table_source: source.clone(),
            source,
            mode: Mode::All,
            pager: Pager::new(total),
            continuous: false,
            find,
            filter,
            filter_visible: false,
            editor,
            editing: None,
            edits: Default::default(),
            table_edits: Default::default(),
            grid_is_query: false,
            find_reset: false,
            cell_cursor: None,
            viewport_rows: 10,
            palette_input,
            palette_open: false,
            detail_open: false,
            catalog: Catalog::synthetic(),
            live: None,
            connections: Vec::new(),
            keymap: Keymap::default(),
            palette_index: 0,
            dark_mode: Theme::current(cx).dark,
            tabs: Tabs::default(),
            tab_scroll: ScrollHandle::new(),
            runs: vec![Run {
                number: 1,
                rows: total,
                elapsed_ms: None,
            }],
            run_edits: vec![grid_edit::Edits::new()],
            run_sources: vec![None],
            sql_input,
            active_run: 0,
            run_scroll: ScrollHandle::new(),
            next_run_number: 2,
            result_view: ResultView::Table,
            messages: MessageLog::default(),
            result_ratio: DEFAULT_RESULT_RATIO,
            result_open: true,
            resizing_split: false,
            font: default_font(&font_names).into(),
            font_names,
            palette_fonts: false,
            palette_address: None,
            palette_pattern: None,
            check_histories: Vec::new(),
            sidebar_open: true,
            import_menu_open: false,
            sidebar_width: 340.0,
            resizing_sidebar: false,
            connection_open: true,
            database_open: true,
            sidebar_search,
            cursor: MatchCursor::default(),
            find_capped: false,
            operation_cancel: CancellationToken::new(),
            compare: None,
            status: String::new(),
            selection: None,
            generation: 0,
            scroll: UniformListScrollHandle::new(),
            focus: table_focus,
            started,
            first_frame_reported: false,
            last_frame: None,
            frame_intervals: Vec::new(),
            measuring: false,
            pace: None,
        }
    }

    fn dataset_count(&self) -> u64 {
        if matches!(self.tabs.current(), Tab::Query) && self.runs.is_empty() {
            return 0;
        }
        match &self.mode {
            Mode::All => self.source.total(),
            Mode::Filtered { kept, .. } => kept.len() as u64,
        }
    }

    fn window(&self) -> (u64, u64) {
        if self.continuous {
            (0, self.dataset_count())
        } else {
            self.pager.window()
        }
    }

    fn base(&self) -> u64 {
        self.window().0
    }

    fn visible_count(&self) -> usize {
        if let Some(diff) = &self.compare {
            return diff.rows.len();
        }
        let (start, end) = self.window();
        (end - start) as usize
    }

    fn source_row(&self, dataset_index: u64) -> u64 {
        match &self.mode {
            Mode::All => dataset_index,
            Mode::Filtered { kept, .. } => u64::from(kept[dataset_index as usize]),
        }
    }

    fn dataset_index_of(&self, source: u64) -> Option<u64> {
        match &self.mode {
            Mode::All => Some(source),
            Mode::Filtered { kept, .. } => kept.binary_search(&(source as u32)).ok().map(|index| index as u64),
        }
    }

    fn click_row(&mut self, dataset_index: u64, shift: bool) {
        self.selection = Some(Selection::after_click(self.selection, dataset_index, shift));
        self.status = self.selection.map(Selection::label).unwrap_or_default();
    }

    fn selected_rows(&self, selection: Selection) -> Vec<Vec<Cell>> {
        (selection.first()..=selection.last())
            .map(|index| {
                let source = self.source_row(index);
                let mut row = self.source.row(source);
                for (column, cell) in row.iter_mut().enumerate() {
                    if let Some(edited) = self.edits.get(&(source, column)) {
                        *cell = edited.clone();
                    }
                }
                row
            })
            .collect()
    }

    fn copy_selection(&mut self, with_headers: bool, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else { return };
        if !selection.copyable() {
            self.status = selection.label();
            return;
        }
        let text = copy_tsv(self.selected_rows(selection), with_headers);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let noun = if selection.count() == 1 { "row" } else { "rows" };
        self.status = format!(
            "Copied {} {noun}{}",
            selection.count(),
            if with_headers { " with headers" } else { "" }
        );
    }

    fn copy_selection_as(&mut self, format: CopyFormat, cx: &mut Context<Self>) {
        if format == CopyFormat::Insert && (self.grid_is_query || self.catalog.tables.is_empty()) {
            self.status = "Copy as INSERT is available for table rows only.".into();
            return;
        }
        let Some(selection) = self.selection else { return };
        if !selection.copyable() {
            self.status = selection.label();
            return;
        }
        let rows = self.selected_rows(selection);
        let driver_id = self
            .live
            .as_ref()
            .map_or_else(|| "postgres".to_string(), |session| session.driver_id.clone());
        let context = CopyContext {
            columns: self.source.columns(),
            driver_id: &driver_id,
            table: &self.catalog.grid_table,
            column: self.cell_cursor.map_or(0, |cursor| cursor.column),
        };
        match copy_format::render(format, &context, &rows) {
            Ok(copied) => {
                cx.write_to_clipboard(ClipboardItem::new_string(copied.text));
                let noun = if selection.count() == 1 { "row" } else { "rows" };
                self.status = match copied.note {
                    Some(note) => format!("Copied {} {noun} ({note})", selection.count()),
                    None => format!("Copied {} {noun}", selection.count()),
                };
            }
            Err(message) => self.status = message,
        }
    }

    fn sample_pubsub(&mut self, pattern: String, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else {
            return;
        };
        self.status = format!(
            "Listening to {pattern} for {} s…",
            bookie_gpui::pubsub_view::SAMPLE_SECONDS
        );
        let target = pattern.clone();
        self.begin_background(
            cx,
            move |cancel| {
                live.sample_pubsub(
                    std::slice::from_ref(&target),
                    bookie_gpui::pubsub_view::SAMPLE_MESSAGES,
                    Duration::from_secs(bookie_gpui::pubsub_view::SAMPLE_SECONDS),
                    &cancel,
                )
            },
            move |this, outcome| {
                let lines = bookie_gpui::pubsub_view::describe(&pattern, outcome);
                if let Some((_, headline)) = lines.first() {
                    this.status = headline.clone();
                }
                for (kind, text) in lines {
                    this.messages.push(kind, text);
                }
            },
        );
    }

    fn kafka_topics(&mut self, broker: String, cx: &mut Context<Self>) {
        self.status = format!("Listing topics at {broker}…");
        self.begin_background(
            cx,
            move |_| bookie_gpui::net_check::kafka_topics(&broker),
            |this, outcome| {
                let (kind, text) = match outcome {
                    Ok(text) => (MessageKind::Success, text),
                    Err(error) => (MessageKind::Error, format!("Kafka · {error}")),
                };
                this.messages.push(kind, text.clone());
                this.status = text;
            },
        );
    }

    fn check_tcp(&mut self, address: String, cx: &mut Context<Self>) {
        self.status = format!("Checking {address}…");
        let target = address.clone();
        self.begin_background(
            cx,
            move |_| bookie_gpui::net_check::run_check(&target),
            move |this, outcome| {
                let (up, text) = bookie_gpui::net_check::record(&mut this.check_histories, &address, outcome);
                let kind = if up { MessageKind::Success } else { MessageKind::Error };
                this.messages.push(kind, text.clone());
                this.status = text;
            },
        );
    }

    fn show_status(&mut self) {
        let Some(live) = self.live.clone() else {
            return;
        };
        let lines = bookie_gpui::status_view::describe(&live.overview());
        if let Some((_, headline)) = lines.first() {
            self.status = headline.clone();
        }
        for (kind, text) in lines {
            self.messages.push(kind, text);
        }
    }

    fn show_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.open_query();
        self.sync_grid_owner();
        self.sql_input.update(cx, |input, cx| {
            input.set_value(bookie_app_services::pg_sessions::SESSIONS_SQL, window, cx)
        });
        self.run_query(cx);
    }

    fn run_query(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.live.clone() {
            let sql = self.sql_input.read(cx).value().to_string();
            if sql.trim().is_empty() {
                self.status = "Type a statement first".into();
                return;
            }
            self.status = "Running…".into();
            self.begin_background(
                cx,
                {
                    let statement = sql.clone();
                    move |cancel| {
                        let started = Instant::now();
                        let result = session.run_query_controlled(&statement, bookie_gpui::live::ROW_CAP, &cancel);
                        (statement, started.elapsed(), result)
                    }
                },
                |this, (sql, elapsed, result)| match result {
                    Ok(table) => {
                        let (rows, truncated) = (bookie_gpui::source::ResultSource::total(&table), table.truncated);
                        this.messages.statement_ok(&sql, rows, truncated, elapsed);
                        this.add_run(Some(std::sync::Arc::new(table)));
                        if let Some(run) = this.runs.last_mut() {
                            run.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
                        }
                    }
                    Err(message) => {
                        this.messages.statement_failed(&sql, &message, elapsed);
                        this.result_view = ResultView::Messages;
                        this.status = message;
                    }
                },
            );
            return;
        }
        self.add_run(None);
    }

    fn add_run(&mut self, source: Option<Source>) {
        let mut run = next_run(self.next_run_number);
        if let Some(source) = &source {
            run.rows = source.total();
        }
        self.next_run_number += 1;
        self.runs.push(run);
        self.run_sources.push(source);
        self.run_edits.push(grid_edit::Edits::new());
        self.activate_run(self.runs.len() - 1);
    }

    fn close_tab(&mut self, index: usize) {
        let closing_query = matches!(self.tabs.items.get(index), Some(Tab::Query));
        self.tabs.close(index);
        self.sync_grid_owner();
        if !closing_query {
            return;
        }
        if self.grid_is_query {
            self.invalidate_background();
            self.edits = std::mem::take(&mut self.table_edits);
            self.source = self.table_source.clone();
            self.grid_is_query = false;
            self.editing = None;
            self.compare = None;
            self.cursor = MatchCursor::default();
            self.set_mode(Mode::All);
        }
        self.runs.clear();
        self.run_sources.clear();
        self.run_edits.clear();
        self.active_run = 0;
        self.next_run_number = 1;
    }

    fn sync_grid_owner(&mut self) {
        self.tab_scroll.scroll_to_item(self.tabs.active);
        let query = match self.tabs.current() {
            Tab::Query => true,
            Tab::Table(name) if *name == self.catalog.grid_table => false,
            _ => {
                self.invalidate_background();
                return;
            }
        };
        if query == self.grid_is_query {
            return;
        }
        self.invalidate_background();
        self.editing = None;
        self.compare = None;
        self.cursor = MatchCursor::default();
        self.find_reset = true;
        if query {
            self.table_edits = std::mem::take(&mut self.edits);
            if let Some(run) = self.runs.get(self.active_run).copied() {
                self.show_run(self.active_run, run);
            } else {
                self.source = self.table_source.clone();
                self.set_mode(Mode::All);
            }
        } else {
            if let Some(edits) = self.run_edits.get_mut(self.active_run) {
                *edits = std::mem::take(&mut self.edits);
            }
            self.edits = std::mem::take(&mut self.table_edits);
            self.source = self.table_source.clone();
            self.set_mode(Mode::All);
        }
        self.grid_is_query = query;
    }

    fn activate_run(&mut self, index: usize) {
        let Some(run) = self.runs.get(index).copied() else {
            return;
        };
        self.invalidate_background();
        if let Some(previous) = self.run_edits.get_mut(self.active_run) {
            *previous = std::mem::take(&mut self.edits);
        }
        self.show_run(index, run);
    }

    fn select_run(&mut self, index: usize) {
        if index != self.active_run {
            self.activate_run(index);
        }
    }

    fn show_run(&mut self, index: usize, run: Run) {
        self.run_scroll.scroll_to_item(index);
        self.edits = std::mem::take(&mut self.run_edits[index]);
        self.editing = None;
        self.active_run = index;
        self.source = self
            .run_sources
            .get(index)
            .cloned()
            .flatten()
            .unwrap_or_else(|| std::sync::Arc::new(Synthetic::new(run.rows)));
        self.compare = None;
        self.cursor = MatchCursor::default();
        self.set_mode(Mode::All);
        self.status = format!("{} · {} rows", run.label(), thousands(run.rows));
    }

    fn close_run(&mut self, index: usize) {
        if index >= self.runs.len() {
            return;
        }
        self.runs.remove(index);
        self.run_sources.remove(index);
        self.run_edits.remove(index);
        if index < self.active_run {
            self.active_run -= 1;
            return;
        }
        if index != self.active_run {
            return;
        }
        self.invalidate_background();
        self.edits.clear();
        if self.runs.is_empty() {
            self.source = self.table_source.clone();
            self.editing = None;
            self.compare = None;
            self.cursor = MatchCursor::default();
            self.set_mode(Mode::All);
            self.status = "No results".into();
            return;
        }
        let next = index.min(self.runs.len() - 1);
        self.show_run(next, self.runs[next]);
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.pager = self.pager.with_total(self.dataset_count());
        self.selection = None;
        self.cell_cursor = None;
        self.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
    }

    fn note_frame(&mut self) {
        let now = Instant::now();
        if !self.first_frame_reported {
            self.first_frame_reported = true;
            println!("PROTOTYPE_FIRST_FRAME ms={}", self.started.elapsed().as_millis());
        }
        if let (true, Some(last)) = (self.measuring, self.last_frame) {
            self.frame_intervals.push(now - last);
        }
        self.last_frame = Some(now);
    }

    fn begin_background<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(CancellationToken) -> T + Send + 'static,
        apply: impl FnOnce(&mut Self, T) + 'static,
    ) {
        self.invalidate_background();
        let generation = self.generation;
        let cancel = CancellationToken::new();
        self.operation_cancel = cancel.clone();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { work(cancel) }).await;
            this.update(cx, |this, cx| {
                if this.generation == generation {
                    apply(this, result);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn run_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.dataset_count() == 0 || self.find_text(cx).is_empty() {
            return;
        }
        if !self.cursor.matches.is_empty() && self.cursor.status(false) != "No match" {
            self.step_match(forward);
            return;
        }
        let (source, query) = (self.source.clone(), self.find_text(cx));
        self.status = "Searching…".into();
        self.begin_background(
            cx,
            move |cancel| source.find(&query, MAX_FIND_MATCHES, &cancel),
            move |this, found| match found {
                Ok(found) => {
                    this.find_capped = found.capped;
                    this.cursor.set(found.matches);
                    this.step_match(forward);
                }
                Err(message) => this.status = message.into(),
            },
        );
    }

    fn step_match(&mut self, forward: bool) {
        if let Some(row) = self.cursor.step(forward) {
            let Some(at) = self.dataset_index_of(row) else {
                self.status = "Match is outside the filtered rows".into();
                return;
            };
            self.selection = Some(Selection::after_click(None, at, false));
            if !self.continuous {
                self.pager.contains_row(at);
            }
            let local = at.saturating_sub(self.base()) as usize;
            self.scroll.scroll_to_item_strict(local, ScrollStrategy::Center);
        }
        self.status = self.cursor.status(self.find_capped);
    }

    fn find_text(&self, cx: &App) -> String {
        self.find.read(cx).content().to_string()
    }

    fn show_matching(&mut self, cx: &App) {
        if self.cursor.matches.is_empty() {
            return;
        }
        let kept = self.cursor.matches.iter().map(|row| *row as u32).collect();
        let label = format!("FILTER {}", self.find_text(cx));
        self.set_mode(Mode::Filtered {
            kept: std::sync::Arc::new(kept),
            label,
        });
        self.status = format!("Showing {} matching rows", thousands(self.dataset_count()));
    }

    fn apply_filter(&mut self, cx: &mut Context<Self>) {
        if matches!(self.tabs.current(), Tab::Query) && self.runs.is_empty() {
            return;
        }
        let predicate = match parse_predicate(self.filter.read(cx).content(), self.source.columns()) {
            Ok(predicate) => predicate,
            Err(message) => {
                self.status = message.into();
                return;
            }
        };
        let source = self.source.clone();
        let label = format!("WHERE {}", self.filter.read(cx).content());
        self.status = "Filtering…".into();
        self.begin_background(
            cx,
            move |cancel| source.filter(&predicate, &cancel),
            move |this, kept| {
                let kept = match kept {
                    Ok(kept) => kept,
                    Err(message) => {
                        this.status = message.into();
                        return;
                    }
                };
                this.set_mode(Mode::Filtered {
                    kept: std::sync::Arc::new(kept),
                    label,
                });
                this.status = format!("Showing {} matching rows", thousands(this.dataset_count()));
            },
        );
    }

    fn clear_filter(&mut self) {
        self.invalidate_background();
        self.cursor = MatchCursor::default();
        self.set_mode(Mode::All);
        self.status.clear();
    }

    fn compare(&mut self, keyed: bool, cx: &mut Context<Self>) {
        if matches!(self.tabs.current(), Tab::Query) && self.runs.is_empty() {
            return;
        }
        let source = self.source.clone();
        self.status = "Comparing…".into();
        self.begin_background(
            cx,
            move |cancel| source.compare(keyed, &cancel),
            |this, result| match result {
                Ok(diff) => {
                    this.status.clear();
                    this.selection = None;
                    this.compare = Some(std::sync::Arc::new(diff));
                    this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                }
                Err(message) => this.status = message.into(),
            },
        );
    }

    fn invalidate_background(&mut self) {
        self.generation += 1;
        self.operation_cancel.cancel();
    }

    fn next_page(&mut self) -> bool {
        let moved = !self.continuous && self.pager.page + 1 < self.pager.pages();
        if moved {
            self.pager.next();
        }
        moved
    }

    fn previous_page(&mut self) -> bool {
        let moved = !self.continuous && self.pager.page > 0 && !self.pager.append;
        if moved {
            self.pager.previous();
        }
        moved
    }

    fn place_cursor(&mut self, cursor: Option<Cursor>) {
        let previous = self.cell_cursor.map_or(0, |c| c.row);
        self.cell_cursor = cursor;
        let Some(cursor) = cursor else { return };
        let strategy = if cursor.row < previous {
            ScrollStrategy::Top
        } else {
            ScrollStrategy::Bottom
        };
        self.scroll.scroll_to_item(cursor.row, strategy);
        self.status = cursor.label(&self.column_names());
    }

    fn column_names(&self) -> Vec<String> {
        self.source.columns().iter().map(|column| column.name.clone()).collect()
    }

    fn palette_actions(&self) -> Vec<Action> {
        if self.palette_fonts {
            return self
                .font_names
                .iter()
                .map(|name| Action {
                    id: "font-choice",
                    label: name.clone(),
                    enabled: true,
                })
                .collect();
        }
        let filtering = matches!(self.mode, Mode::Filtered { .. });
        let paged = !self.continuous;
        let check = self.palette_address.as_ref().map(|address| Action {
            id: "check-tcp",
            label: format!("Check TCP port {address}"),
            enabled: true,
        });
        let listen = self.palette_pattern.as_ref().map(|pattern| Action {
            id: "sample-pubsub",
            label: format!("Listen to Redis channels {pattern}"),
            enabled: true,
        });
        let action = |id, label: &str, enabled| Action {
            id,
            label: label.to_string(),
            enabled,
        };
        check
            .into_iter()
            .chain(listen)
            .chain(vec![
                action("first", "Go to first page", paged),
                action("previous", "Go to previous page", paged && self.pager.page > 0),
                action(
                    "next",
                    "Go to next page",
                    paged && self.pager.page + 1 < self.pager.pages(),
                ),
                action("last", "Go to last page", paged),
                action("find", "Find in all rows", !filtering),
                action("filter", "Filter rows (WHERE)", true),
                action("clear-filter", "Clear filter", filtering),
                action("compare", "Compare results", true),
                action(
                    "detail",
                    if self.detail_open {
                        "Hide cell details"
                    } else {
                        "Show cell details"
                    },
                    true,
                ),
                action(
                    "continuous",
                    if self.continuous {
                        "Switch to paged scrolling"
                    } else {
                        "Switch to continuous scrolling"
                    },
                    true,
                ),
                action("copy", "Copy selected rows", self.selection.is_some()),
                action("copy-csv", "Copy selected rows as CSV", self.selection.is_some()),
                action("copy-json", "Copy selected rows as JSON", self.selection.is_some()),
                action(
                    "copy-markdown",
                    "Copy selected rows as a Markdown table",
                    self.selection.is_some(),
                ),
                action(
                    "copy-in-list",
                    "Copy the active column as an IN list",
                    self.selection.is_some(),
                ),
                action(
                    "copy-insert",
                    "Copy selected rows as INSERT statements",
                    self.selection.is_some() && !self.grid_is_query && !self.catalog.tables.is_empty(),
                ),
                action(
                    "sessions",
                    "Show PostgreSQL sessions and blockers",
                    self.live.as_ref().is_some_and(|live| live.driver_id == "postgres"),
                ),
                action("status", "Show connection and tunnel status", self.live.is_some()),
                action("clear-selection", "Clear row selection", self.selection.is_some()),
                action("edit", "Edit the active cell", self.cell_cursor.is_some()),
                action(
                    "discard-view-edits",
                    "Discard all view-only edits",
                    self.has_view_edits(),
                ),
                action(
                    "sidebar",
                    if self.sidebar_open {
                        "Hide the sidebar"
                    } else {
                        "Show the sidebar"
                    },
                    true,
                ),
                action("font", "Choose font…", true),
                action("theme-dbx", "Theme: DBX style", true),
                action("theme-adwaita", "Theme: Adwaita style (our GTK app)", true),
                action("query", "New query tab", true),
                action("run", "Run the query", matches!(self.tabs.current(), Tab::Query)),
                action(
                    "previous-run",
                    "Previous result",
                    matches!(self.tabs.current(), Tab::Query) && !self.runs.is_empty() && self.active_run > 0,
                ),
                action(
                    "next-run",
                    "Next result",
                    matches!(self.tabs.current(), Tab::Query) && self.active_run + 1 < self.runs.len(),
                ),
                action(
                    "close-run",
                    "Close current result",
                    matches!(self.tabs.current(), Tab::Query) && !self.runs.is_empty(),
                ),
                action(
                    "results",
                    if self.result_open {
                        "Hide the results pane"
                    } else {
                        "Show the results pane"
                    },
                    matches!(self.tabs.current(), Tab::Query),
                ),
                action("home", "Open Home", true),
                action("settings", "Open settings", true),
                action(
                    "table",
                    &format!("Open {}", self.catalog.grid_table),
                    !self.catalog.tables.is_empty(),
                ),
                action(
                    "theme",
                    if self.dark_mode {
                        "Switch to light theme"
                    } else {
                        "Switch to dark theme"
                    },
                    true,
                ),
            ])
            .collect()
    }

    fn palette_shown(&self, cx: &App) -> Vec<usize> {
        palette_matches(self.palette_input.read(cx).content(), &self.palette_actions())
    }

    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_open = true;
        self.palette_fonts = false;
        self.palette_index = 0;
        self.palette_input.update(cx, |input, cx| input.set_content("", cx));
        window.focus(&self.palette_input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_open = false;
        self.palette_fonts = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn run_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let actions = self.palette_actions();
        let shown = palette_matches(self.palette_input.read(cx).content(), &actions);
        let Some(index) = shown
            .get(self.palette_index.min(shown.len().saturating_sub(1)))
            .copied()
        else {
            return;
        };
        let id = actions[index].id;
        let check_address = self.palette_address.clone();
        let listen_pattern = self.palette_pattern.clone();
        if id == "font" {
            self.palette_fonts = true;
            self.palette_index = 0;
            self.palette_input.update(cx, |input, cx| input.set_content("", cx));
            cx.notify();
            return;
        }
        if id == "font-choice" {
            self.font = actions[index].label.clone().into();
            self.close_palette(window, cx);
            return;
        }
        self.close_palette(window, cx);
        match id {
            "first" => self.pager.first(),
            "previous" => self.pager.previous(),
            "next" => self.pager.next(),
            "last" => self.pager.last(),
            "find" => window.focus(&self.find.read(cx).focus_handle(cx), cx),
            "filter" => {
                self.filter_visible = true;
                window.focus(&self.filter.read(cx).focus_handle(cx), cx);
            }
            "clear-filter" => self.clear_filter(),
            "compare" => self.compare(false, cx),
            "detail" => self.toggle_detail(),
            "continuous" => {
                self.continuous = !self.continuous;
                self.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
            }
            "copy" => self.copy_selection(false, cx),
            "copy-csv" => self.copy_selection_as(CopyFormat::Csv, cx),
            "copy-json" => self.copy_selection_as(CopyFormat::Json, cx),
            "copy-markdown" => self.copy_selection_as(CopyFormat::Markdown, cx),
            "copy-in-list" => self.copy_selection_as(CopyFormat::InList, cx),
            "copy-insert" => self.copy_selection_as(CopyFormat::Insert, cx),
            "discard-view-edits" => self.discard_view_edits(),
            "sessions" => self.show_sessions(window, cx),
            "kafka-topics" => {
                if let Some(broker) = check_address.clone() {
                    self.kafka_topics(broker, cx);
                }
            }
            "status" => self.show_status(),
            "sample-pubsub" => {
                if let Some(pattern) = listen_pattern {
                    self.sample_pubsub(pattern, cx);
                }
            }
            "check-tcp" => {
                if let Some(address) = check_address {
                    self.check_tcp(address, cx);
                }
            }
            "clear-selection" => self.selection = None,
            "edit" => self.grid_key("enter", false, false, window, cx),
            "sidebar" => self.sidebar_open = !self.sidebar_open,
            "settings" => {
                self.tabs.open_settings();
                self.sync_grid_owner();
            }
            "home" => {
                self.tabs.open_home();
                self.sync_grid_owner();
            }
            "query" => {
                self.tabs.open_query();
                self.sync_grid_owner();
                window.focus(&self.focus, cx);
            }
            "run" => self.run_query(cx),
            "previous-run" => self.activate_run(self.active_run - 1),
            "next-run" => self.activate_run(self.active_run + 1),
            "close-run" => self.close_run(self.active_run),
            "results" => self.result_open = !self.result_open,
            "table" => {
                let name = self.catalog.grid_table.clone();
                self.open_table_named(&name, window, cx);
            }
            "theme-dbx" | "theme-adwaita" => {
                let style = if id == "theme-dbx" { Style::Dbx } else { Style::Adwaita };
                Theme::current(cx).with_style(style).install(cx);
                window.refresh();
            }
            "theme" => {
                self.dark_mode = !self.dark_mode;
                Theme::current(cx).toggled().install(cx);
                window.refresh();
            }
            _ => {}
        }
        cx.notify();
    }

    fn pace_frame(&mut self, window: &mut Window) {
        let last = self.visible_count().saturating_sub(1);
        let Some(pace) = self.pace.as_mut() else { return };
        let target = (f64::from(pace.step) / f64::from(pace.steps) * last as f64) as usize;
        self.scroll.scroll_to_item_strict(target, ScrollStrategy::Top);
        pace.step += 1;
        if pace.step <= pace.steps {
            window.request_animation_frame();
            return;
        }
        let elapsed = pace.begun.elapsed().as_millis();
        let steps = pace.steps;
        self.pace = None;
        self.measuring = false;
        self.report_scroll(steps, elapsed, true);
    }

    fn report_scroll(&self, steps: u32, elapsed: u128, with_frames: bool) {
        let mut sorted: Vec<f64> = self.frame_intervals.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
        sorted.sort_by(f64::total_cmp);
        let pick = |q: f64| {
            sorted
                .get(((sorted.len() as f64 - 1.0) * q) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let frames = if with_frames {
            format!(
                " frames={} frame_p50_ms={:.2} frame_p95_ms={:.2} frame_p99_ms={:.2} frame_max_ms={:.2}",
                sorted.len(),
                pick(0.5),
                pick(0.95),
                pick(0.99),
                pick(1.0)
            )
        } else {
            String::new()
        };
        println!("PROTOTYPE_SCROLL_DONE steps={steps} elapsed_ms={elapsed}{frames}");
    }

    fn start_autoscroll(&mut self, steps: u32, cx: &mut Context<Self>) {
        if std::env::var("BOOKIE_PROTOTYPE_AUTOSCROLL_SCOPE").as_deref() == Ok("continuous") {
            self.continuous = true;
        }
        let frame_paced = std::env::var("BOOKIE_PROTOTYPE_AUTOSCROLL_PACING").as_deref() == Ok("frame");
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            if frame_paced {
                this.update(cx, |this, cx| {
                    this.frame_intervals.clear();
                    this.last_frame = None;
                    this.measuring = true;
                    this.pace = Some(Pace {
                        step: 0,
                        steps,
                        begun: Instant::now(),
                    });
                    cx.notify();
                })
                .ok();
                return;
            }
            let begun = Instant::now();
            for step in 0..=steps {
                let moved = this.update(cx, |this, cx| {
                    let last = this.visible_count().saturating_sub(1);
                    let target = (f64::from(step) / f64::from(steps) * last as f64) as usize;
                    this.scroll.scroll_to_item_strict(target, ScrollStrategy::Top);
                    cx.notify();
                });
                if moved.is_err() {
                    return;
                }
                cx.background_executor().timer(Duration::from_millis(62)).await;
            }
            let elapsed = begun.elapsed().as_millis();
            this.update(cx, |this, _| this.report_scroll(steps, elapsed, false))
                .ok();
        })
        .detach();
    }
}

fn button(id: &'static str, label: impl Into<String>, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(24.0))
        .px_2()
        .rounded(px(4.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.background))
        .text_color(rgb(theme.foreground))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(label.into())
}

pub fn flex_cell(width: f32) -> Div {
    div()
        .flex()
        .items_center()
        .h_full()
        .px_2()
        .flex_basis(px(width))
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .min_w(px(width * 0.4))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
}

pub fn fixed_cell(width: f32, text: String) -> Div {
    div()
        .flex()
        .items_center()
        .h_full()
        .px_2()
        .w(px(width))
        .flex_none()
        .overflow_hidden()
        .child(text)
}

pub fn cell_div(width: f32, text: String) -> Div {
    flex_cell(width).child(text)
}

/// A number is right-aligned; so is a NULL in a numeric column, so it lines up with its neighbours.
fn right_aligned(numeric_column: bool, cell: &Cell) -> bool {
    numeric_cell(cell) || (numeric_column && matches!(cell, Cell::Null))
}

fn numeric_cell(cell: &Cell) -> bool {
    use bookie_gpui::type_colors::{TypeKind, type_kind};
    matches!(type_kind(cell), Some(TypeKind::Integer | TypeKind::Numeric))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capped_load_is_marked_with_a_plus_in_the_title() {
        assert_eq!(render::progress_text("", 100_000, true, 20), "100,000+ rows · 20 pages");
        assert_eq!(render::progress_text("", 40, false, 1), "40 rows");
        assert_eq!(render::progress_text("label · ", 1_500, true, 1), "label · 1,500+ rows");
    }

    #[test]
    fn only_numeric_cells_use_right_alignment() {
        assert!(numeric_cell(&Cell::Int(7)));
        assert!(numeric_cell(&Cell::Float(7.5)));
        assert!(numeric_cell(&Cell::Decimal("7.50".parse().unwrap())));
        assert!(numeric_cell(&bookie_core::Value::Decimal("7.50".parse().unwrap())));
        assert!(!numeric_cell(&Cell::Text("7".into())));
        assert!(!numeric_cell(&Cell::Null));
    }

    #[test]
    fn a_null_in_a_numeric_column_aligns_with_the_numbers() {
        assert!(right_aligned(true, &Cell::Null));
        assert!(!right_aligned(false, &Cell::Null));
        assert!(!right_aligned(true, &Cell::Text("7".into())));
    }
}

fn open_requested_start_tab(view: &Entity<GridView>, name: &str, cx: &mut App) {
    view.update(cx, |view, _| {
        match name {
            "settings" => view.tabs.open_settings(),
            "query" => view.tabs.open_query(),
            _ => return,
        }
        view.sync_grid_owner();
    });
}

fn main() {
    let started = Instant::now();
    let total = rows_from(std::env::var("BOOKIE_PROTOTYPE_ROWS").ok().as_deref());
    let live = live_source();
    let keymap = load_keymap();
    let connections = if live.is_some() {
        Vec::new()
    } else {
        bookie_gpui::live::saved_connections().unwrap_or_else(|error| {
            eprintln!("saved connections unavailable: {error}");
            Vec::new()
        })
    };
    let autoscroll: u32 = env_number("BOOKIE_PROTOTYPE_AUTOSCROLL").unwrap_or(0);
    let width: f32 = env_number("BOOKIE_PROTOTYPE_WIDTH").unwrap_or(1100.0);
    let height: f32 = env_number("BOOKIE_PROTOTYPE_HEIGHT").unwrap_or(800.0);
    gpui_platform::application()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            text_input::bind_keys(cx);
            Theme::named(std::env::var("BOOKIE_PROTOTYPE_THEME").ok().as_deref()).install(cx);
            let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| match live.clone() {
                        Some(table) => {
                            let mut view = GridView::with_source(table.clone(), started, window, cx);
                            view.attach_live(table);
                            view
                        }
                        None => GridView::new(total, started, window, cx),
                    });
                    let focus = view.read(cx).focus.clone();
                    focus.focus(window, cx);
                    view.update(cx, |view, _| {
                        view.keymap = keymap.clone();
                        view.connections = connections.clone();
                    });
                    if let Ok(start_tab) = std::env::var("BOOKIE_PROTOTYPE_START") {
                        open_requested_start_tab(&view, &start_tab, cx);
                    }
                    if autoscroll > 0 {
                        view.update(cx, |view, cx| view.start_autoscroll(autoscroll, cx));
                    }
                    view
                },
            );
            if let Err(error) = opened {
                eprintln!("could not open a window: {error}");
                std::process::exit(2);
            }
        });
}

#[cfg(test)]
mod live_ui_tests;
#[cfg(test)]
mod ui_tests;

fn load_keymap() -> Keymap {
    let Some(path) = bookie_app_services::config_io::xdg_config_path("gpui-keymap.json") else {
        return Keymap::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Keymap::default();
    };
    Keymap::from_overrides(&text).unwrap_or_else(|error| {
        eprintln!("ignoring {}: {error}", path.display());
        Keymap::default()
    })
}

fn live_source() -> Option<std::sync::Arc<bookie_gpui::live::LiveTable>> {
    use bookie_gpui::live::{LiveTable, Origin};
    let table = std::env::var("BOOKIE_PROTOTYPE_TABLE").ok()?;
    let origin = match (
        std::env::var("BOOKIE_PROTOTYPE_FILE").ok(),
        std::env::var("BOOKIE_PROTOTYPE_CONNECTION").ok(),
    ) {
        (Some(path), _) => Origin::File(path.into()),
        (None, Some(connection)) => Origin::Saved(connection),
        (None, None) => return None,
    };
    match LiveTable::load(&origin, &table, bookie_gpui::live::ROW_CAP) {
        Ok(live) => Some(std::sync::Arc::new(live)),
        Err(error) => {
            eprintln!("could not read {table}: {error}");
            std::process::exit(2);
        }
    }
}
