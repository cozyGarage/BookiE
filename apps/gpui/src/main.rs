use std::ops::Range;
use std::time::{Duration, Instant};

mod assets;
mod chrome;
mod detail_panel;
mod grid_edit;
mod keys;
mod render;
mod text_input;
mod ui_theme;

use bookie_gpui::cursor::Cursor;
use bookie_gpui::keymap::Keymap;
use bookie_gpui::model::*;
use bookie_gpui::palette::{Action, matches as palette_matches, step as palette_step};
use bookie_gpui::preview::cell_preview;
use bookie_gpui::selection::{Selection, copy_tsv};
use bookie_gpui::shell::{DATA_TABLE, DEFAULT_RESULT_RATIO, Run, Tab, Tabs, next_run, result_ratio_from_drag};
use bookie_gpui::source::{Source, Synthetic};
use bookie_gpui::theme::{Style, Theme};
use gpui::{
    App, Bounds, ClipboardItem, Context, Div, Entity, FocusHandle, Focusable, MouseButton, MouseDownEvent,
    ScrollHandle, ScrollStrategy, Stateful, UniformListScrollHandle, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size, uniform_list,
};
use text_input::{InputEvent, TextInput};
use tokio_util::sync::CancellationToken;
use ui_theme::ThemeExt;

const ROW_HEIGHT: f32 = 26.0;
const HEADER_HEIGHT: f32 = 28.0;
pub const GUTTER_WIDTH: f32 = 56.0;

enum Mode {
    All,
    Filtered { kept: Vec<u32>, label: String },
}

struct GridView {
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
    cell_cursor: Option<Cursor>,
    viewport_rows: usize,
    palette_input: Entity<TextInput>,
    palette_open: bool,
    detail_open: bool,
    keymap: Keymap,
    palette_index: usize,
    dark_mode: bool,
    tabs: Tabs,
    tab_scroll: ScrollHandle,
    runs: Vec<Run>,
    run_edits: Vec<grid_edit::Edits>,
    active_run: usize,
    run_scroll: ScrollHandle,
    next_run_number: usize,
    summary_view: bool,
    result_ratio: f32,
    result_open: bool,
    resizing_split: bool,
    font: gpui::SharedString,
    font_names: Vec<String>,
    palette_fonts: bool,
    sidebar_open: bool,
    sidebar_width: f32,
    resizing_sidebar: bool,
    connection_open: bool,
    database_open: bool,
    sidebar_search: Entity<TextInput>,
    cursor: MatchCursor,
    find_capped: bool,
    operation_cancel: CancellationToken,
    compare: Option<Diff>,
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
        let find = cx.new(|cx| TextInput::new("Find in all rows", cx));
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
        cx.subscribe(&sidebar_search, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        let palette_input = cx.new(|cx| TextInput::new("Type a command", cx));
        cx.subscribe_in(
            &palette_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Changed => {
                    this.palette_index = 0;
                    cx.notify();
                }
                InputEvent::Submit => this.run_palette(window, cx),
                InputEvent::Cancel => this.close_palette(window, cx),
            },
        )
        .detach();
        GridView {
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
            cell_cursor: None,
            viewport_rows: 10,
            palette_input,
            palette_open: false,
            detail_open: false,
            keymap: Keymap::default(),
            palette_index: 0,
            dark_mode: Theme::current(cx).dark,
            tabs: Tabs::default(),
            tab_scroll: ScrollHandle::new(),
            runs: vec![Run { number: 1, rows: total }],
            run_edits: vec![grid_edit::Edits::new()],
            active_run: 0,
            run_scroll: ScrollHandle::new(),
            next_run_number: 2,
            summary_view: false,
            result_ratio: DEFAULT_RESULT_RATIO,
            result_open: true,
            resizing_split: false,
            font: default_font(&font_names).into(),
            font_names,
            palette_fonts: false,
            sidebar_open: true,
            sidebar_width: 260.0,
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
            focus: cx.focus_handle(),
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

    fn base(&self) -> u64 {
        if self.continuous {
            0
        } else {
            self.pager.page * PAGE_ROWS
        }
    }

    fn visible_count(&self) -> usize {
        if let Some(diff) = &self.compare {
            return diff.rows.len();
        }
        let remaining = self.dataset_count().saturating_sub(self.base());
        if self.continuous {
            remaining as usize
        } else {
            remaining.min(PAGE_ROWS) as usize
        }
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

    fn copy_selection(&mut self, with_headers: bool, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else { return };
        if !selection.copyable() {
            self.status = selection.label();
            return;
        }
        let rows = (selection.first()..=selection.last()).map(|index| {
            let source = self.source_row(index);
            let mut row = self.source.row(source);
            for (column, cell) in row.iter_mut().enumerate() {
                if let Some(edited) = self.edits.get(&(source, column)) {
                    *cell = edited.clone();
                }
            }
            row
        });
        let text = copy_tsv(rows, with_headers);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let noun = if selection.count() == 1 { "row" } else { "rows" };
        self.status = format!(
            "Copied {} {noun}{}",
            selection.count(),
            if with_headers { " with headers" } else { "" }
        );
    }

    fn run_query(&mut self) {
        let run = next_run(self.next_run_number);
        self.next_run_number += 1;
        self.runs.push(run);
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
        self.run_edits.clear();
        self.active_run = 0;
        self.next_run_number = 1;
    }

    fn sync_grid_owner(&mut self) {
        self.tab_scroll.scroll_to_item(self.tabs.active);
        let query = match self.tabs.current() {
            Tab::Query => true,
            Tab::Table(name) if name == DATA_TABLE => false,
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
        self.source = std::sync::Arc::new(Synthetic::new(run.rows));
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
        self.pager = Pager::new(self.dataset_count());
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
        self.set_mode(Mode::Filtered { kept, label });
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
                this.set_mode(Mode::Filtered { kept, label });
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
                    this.compare = Some(diff);
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
        let moved = !self.continuous && self.pager.page > 0;
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
        let action = |id, label: &str, enabled| Action {
            id,
            label: label.to_string(),
            enabled,
        };
        vec![
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
            action("clear-selection", "Clear row selection", self.selection.is_some()),
            action("edit", "Edit the active cell", self.cell_cursor.is_some()),
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
            action("table", "Open synthetic_rows", true),
            action(
                "theme",
                if self.dark_mode {
                    "Switch to light theme"
                } else {
                    "Switch to dark theme"
                },
                true,
            ),
        ]
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
            "clear-selection" => self.selection = None,
            "edit" => self.grid_key("enter", false, false, window, cx),
            "sidebar" => self.sidebar_open = !self.sidebar_open,
            "home" => {
                self.tabs.open_home();
                self.sync_grid_owner();
            }
            "query" => {
                self.tabs.open_query();
                self.sync_grid_owner();
                window.focus(&self.focus, cx);
            }
            "run" => self.run_query(),
            "previous-run" => self.activate_run(self.active_run - 1),
            "next-run" => self.activate_run(self.active_run + 1),
            "close-run" => self.close_run(self.active_run),
            "results" => self.result_open = !self.result_open,
            "table" => {
                self.tabs.open_table(DATA_TABLE);
                self.sync_grid_owner();
                window.focus(&self.focus, cx);
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
        .h(px(28.0))
        .px_3()
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

fn display_cell(width: f32, cell: &Cell, theme: Theme) -> Div {
    let mut item = cell_div(width, cell_preview(cell));
    if let Some(kind) = bookie_gpui::type_colors::type_kind(cell) {
        item = item.text_color(rgb(bookie_gpui::type_colors::type_color(kind, theme.dark)));
    }
    if numeric_cell(cell) { item.justify_end() } else { item }
}

fn numeric_cell(cell: &Cell) -> bool {
    use bookie_gpui_grid_prototype::type_colors::{TypeKind, type_kind};
    matches!(type_kind(cell), Some(TypeKind::Integer | TypeKind::Numeric))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_numeric_cells_use_right_alignment() {
        assert!(numeric_cell(&Cell::Int(7)));
        assert!(numeric_cell(&Cell::Float(7.5)));
        assert!(numeric_cell(&Cell::Decimal("7.50".into())));
        assert!(numeric_cell(&bookie_gpui_grid_prototype::core_bridge::value_to_cell(
            bookie_core::Value::Decimal("7.50".parse().unwrap())
        )));
        assert!(!numeric_cell(&Cell::Text("7".into())));
        assert!(!numeric_cell(&Cell::Null));
    }
}

fn row_div(stripe: bool, selected: bool, theme: Theme) -> Div {
    let background = if selected {
        theme.active_row
    } else if stripe {
        theme.stripe
    } else {
        theme.background
    };
    div()
        .flex()
        .items_center()
        .h(px(ROW_HEIGHT))
        .w_full()
        .bg(rgb(background))
}

fn main() {
    let started = Instant::now();
    let total = rows_from(std::env::var("BOOKIE_PROTOTYPE_ROWS").ok().as_deref());
    let live = live_source();
    let keymap = load_keymap();
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
                        Some(source) => GridView::with_source(source, started, window, cx),
                        None => GridView::new(total, started, window, cx),
                    });
                    let focus = view.read(cx).focus.clone();
                    focus.focus(window, cx);
                    view.update(cx, |view, _| view.keymap = keymap.clone());
                    if std::env::var("BOOKIE_PROTOTYPE_START").as_deref() == Ok("query") {
                        view.update(cx, |view, _| {
                            view.tabs.open_query();
                            view.sync_grid_owner();
                        });
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

const LIVE_ROW_CAP: u64 = 100_000;

fn live_source() -> Option<Source> {
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
    match LiveTable::load(&origin, &table, LIVE_ROW_CAP) {
        Ok(live) => Some(std::sync::Arc::new(live)),
        Err(error) => {
            eprintln!("could not read {table}: {error}");
            std::process::exit(2);
        }
    }
}
