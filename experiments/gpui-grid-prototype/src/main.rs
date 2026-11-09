use std::ops::Range;
use std::time::{Duration, Instant};

mod assets;
mod chrome;
mod grid_edit;
mod text_input;
mod ui_theme;

use bookie_gpui_grid_prototype::cursor::{Cursor, move_cursor};
use bookie_gpui_grid_prototype::model::*;
use bookie_gpui_grid_prototype::palette::{Action, matches as palette_matches, step as palette_step};
use bookie_gpui_grid_prototype::preview::cell_preview;
use bookie_gpui_grid_prototype::selection::{Selection, copy_tsv};
use bookie_gpui_grid_prototype::shell::{
    DATA_TABLE, DEFAULT_RESULT_RATIO, Run, Tab, Tabs, next_run, result_ratio_from_drag,
};
use bookie_gpui_grid_prototype::source::{Source, Synthetic};
use bookie_gpui_grid_prototype::theme::{Style, Theme};
use gpui::{
    App, Bounds, ClipboardItem, Context, Div, Entity, FocusHandle, Focusable, KeyDownEvent, MouseButton,
    MouseDownEvent, ScrollHandle, ScrollStrategy, Stateful, UniformListScrollHandle, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size, uniform_list,
};
use text_input::{InputEvent, TextInput};
use ui_theme::ThemeExt;

const ROW_HEIGHT: f32 = 26.0;
const HEADER_HEIGHT: f32 = 28.0;
pub const GUTTER_WIDTH: f32 = 56.0;
pub const CELL_WIDTHS: [f32; 4] = [240.0, 280.0, 120.0, 320.0];

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
    find_generation: u64,
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
        Self::with_source(std::sync::Arc::new(Synthetic { total }), started, window, cx)
    }

    fn with_source(source: Source, started: Instant, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let total = source.total();
        let find = cx.new(|cx| TextInput::new("Find in all rows", cx));
        let filter = cx.new(|cx| TextInput::new("label contains 7", cx));
        cx.subscribe(&find, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Changed => {
                this.find_generation += 1;
                this.cursor = MatchCursor::default();
                this.status.clear();
                cx.notify();
            }
            InputEvent::Submit => this.run_find(true, cx),
            InputEvent::Cancel => this.status.clear(),
        })
        .detach();
        cx.subscribe(&filter, |this, _, event: &InputEvent, cx| {
            if *event == InputEvent::Submit {
                this.apply_filter(cx);
            }
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
            find_generation: 0,
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
        let rows = (selection.first()..=selection.last()).map(|index| self.source.row(self.source_row(index)));
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

    fn sync_grid_owner(&mut self) {
        self.tab_scroll.scroll_to_item(self.tabs.active);
        let query = match self.tabs.current() {
            Tab::Query => true,
            Tab::Table(name) if name == DATA_TABLE => false,
            _ => return,
        };
        if query == self.grid_is_query {
            return;
        }
        self.generation += 1;
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
        self.generation += 1;
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
        self.source = std::sync::Arc::new(Synthetic { total: run.rows });
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
        self.generation += 1;
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
        work: impl FnOnce() -> T + Send + 'static,
        apply: impl FnOnce(&mut Self, T) + 'static,
    ) {
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { work() }).await;
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
        let find_generation = self.find_generation;
        self.status = "Searching…".into();
        self.begin_background(
            cx,
            move || source.find(&query, MAX_FIND_MATCHES),
            move |this, found| match found {
                _ if this.find_generation != find_generation => {}
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
            let at = self.dataset_index_of(row);
            self.selection = at.map(|index| Selection::after_click(None, index, false));
            if !self.continuous {
                self.pager.contains_row(row);
            }
            let local = row.saturating_sub(self.base()) as usize;
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
        let predicate = match parse_predicate(self.filter.read(cx).content()) {
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
            move || source.filter(&predicate),
            move |this, kept| {
                this.set_mode(Mode::Filtered { kept, label });
                this.status = format!("Showing {} matching rows", thousands(this.dataset_count()));
            },
        );
    }

    fn clear_filter(&mut self) {
        self.generation += 1;
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
            move || source.compare(keyed),
            |this, diff| {
                this.status.clear();
                this.compare = Some(diff);
                this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
            },
        );
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let stroke = &event.keystroke;
        let modifiers = stroke.modifiers;
        let key = stroke.key.as_str();
        if self.palette_open {
            match key {
                "down" | "up" => {
                    let count = self.palette_shown(cx).len();
                    self.palette_index = palette_step(self.palette_index, count, key == "down");
                    cx.notify();
                }
                "escape" => self.close_palette(window, cx),
                _ => {}
            }
            return;
        }
        if modifiers.control && modifiers.shift && key == "p" {
            self.open_palette(window, cx);
            return;
        }
        if modifiers.control {
            match key {
                "f" => {
                    self.filter_visible = true;
                    window.focus(&self.filter.read(cx).focus_handle(cx), cx);
                }
                "c" if self.focus.is_focused(window) => self.copy_selection(modifiers.shift, cx),
                "b" => self.sidebar_open = !self.sidebar_open,
                "t" => {
                    self.tabs.open_query();
                    self.sync_grid_owner();
                    window.focus(&self.focus, cx);
                }
                "enter" if matches!(self.tabs.current(), Tab::Query) => self.run_query(),
                _ => {}
            }
            if !matches!(key, "home" | "end") {
                cx.notify();
                return;
            }
        }
        if self.focus.is_focused(window) && self.editing.is_none() && self.compare.is_none() && self.tabs.shows_grid() {
            self.grid_key(key, modifiers.shift, modifiers.control, window, cx);
        }
    }

    fn grid_key(&mut self, key: &str, shift: bool, ctrl: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.visible_count();
        match key {
            "space" => {
                if let Some(cursor) = self.cell_cursor {
                    self.click_row(self.base() + cursor.row as u64, false);
                }
            }
            "enter" | "f2" => {
                if let Some(cursor) = self.cell_cursor {
                    let source = self.source_row(self.base() + cursor.row as u64);
                    self.start_edit(source, cursor.column, window, cx);
                }
            }
            "down" if self.cell_cursor.is_some_and(|c| c.row + 1 == rows) && self.next_page() => {
                self.place_cursor(Some(Cursor {
                    row: 0,
                    column: self.cell_cursor.map_or(0, |c| c.column),
                }));
            }
            "up" if self.cell_cursor.is_some_and(|c| c.row == 0) && self.previous_page() => {
                let last = self.visible_count().saturating_sub(1);
                self.place_cursor(Some(Cursor {
                    row: last,
                    column: self.cell_cursor.map_or(0, |c| c.column),
                }));
            }
            _ => {
                let Some(next) = move_cursor(self.cell_cursor, key, rows, self.viewport_rows, ctrl) else {
                    return;
                };
                self.place_cursor(Some(next));
                if shift && matches!(key, "up" | "down") {
                    self.click_row(self.base() + next.row as u64, true);
                }
            }
        }
        cx.notify();
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
        self.status = cursor.label();
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

impl Render for GridView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pager.total != self.dataset_count() {
            self.pager = Pager::new(self.dataset_count());
        }
        self.note_frame();
        self.pace_frame(window);
        let theme = Theme::current(cx);
        self.viewport_rows = self.viewport_rows_for(window);
        let (first, last) = self.pager.range();
        let title = match &self.mode {
            Mode::Filtered { label, .. } => format!("{label} · "),
            Mode::All => String::new(),
        };
        let progress = format!(
            "{title}Complete · Received {} rows · {} pages",
            thousands(self.dataset_count()),
            self.pager.pages()
        );
        let range_label = if self.dataset_count() == 0 {
            "Rows 0".to_string()
        } else if self.continuous {
            format!("Rows 1-{}", thousands(self.dataset_count()))
        } else {
            format!("Rows {}-{}", thousands(first), thousands(last))
        };
        let strip = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .p_2()
            .bg(rgb(theme.chrome))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(div().child(progress))
            .child(button("first", "First", theme).on_click(cx.listener(|this, _, _, cx| {
                this.pager.first();
                cx.notify();
            })))
            .child(
                button("prev", "Previous page", theme).on_click(cx.listener(|this, _, _, cx| {
                    this.pager.previous();
                    cx.notify();
                })),
            )
            .child(div().child(range_label))
            .child(
                button("next", "Next page", theme).on_click(cx.listener(|this, _, _, cx| {
                    this.pager.next();
                    cx.notify();
                })),
            )
            .child(button("last", "Last", theme).on_click(cx.listener(|this, _, _, cx| {
                this.pager.last();
                cx.notify();
            })))
            .child(div().w(px(260.0)).child(self.find.clone()))
            .child(
                button("find-prev", "Previous match", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.run_find(false, cx))),
            )
            .child(
                button("find-next", "Next match", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.run_find(true, cx))),
            )
            .child(
                button("show", "Show matching rows", theme).on_click(cx.listener(|this, _, _, cx| {
                    this.show_matching(cx);
                    cx.notify();
                })),
            )
            .child(button("compare", "Compare…", theme).on_click(cx.listener(|this, _, _, cx| this.compare(false, cx))))
            .child(button("export", "Export all rows…", theme))
            .child(
                button(
                    "continuous",
                    if self.continuous {
                        "Continuous scrolling: on"
                    } else {
                        "Continuous scrolling"
                    },
                    theme,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.continuous = !this.continuous;
                    this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                    cx.notify();
                })),
            )
            .child(div().text_color(rgb(theme.muted_foreground)).child(self.status.clone()));
        let filter_row = self.filter_visible.then(|| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .pb_2()
                .child(div().child("WHERE"))
                .child(div().w(px(420.0)).child(self.filter.clone()))
                .child(button("apply", "Apply", theme).on_click(cx.listener(|this, _, _, cx| this.apply_filter(cx))))
                .child(
                    button("clear", "Clear all", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.clear_filter();
                        cx.notify();
                    })),
                )
        });
        let compare_bar = self.compare.as_ref().map(|diff| {
            let s = &diff.summary;
            let summary = if s.keyed {
                format!(
                    "Equal {} · Removed {} · Changed {}",
                    thousands(s.equal),
                    thousands(s.removed),
                    thousands(s.changed)
                )
            } else {
                format!(
                    "Equal {} · Added {} · Removed {}",
                    thousands(s.equal),
                    thousands(s.added),
                    thousands(s.removed)
                )
            };
            let capped = if diff.capped {
                format!(" · first {MAX_DIFF_ROWS} differences shown")
            } else {
                String::new()
            };
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .pb_2()
                .child(div().child(format!("COMPARE {summary}{capped}")))
                .child(
                    button("whole", "Whole row", theme).on_click(cx.listener(|this, _, _, cx| this.compare(false, cx))),
                )
                .child(
                    button("keyed", "Key: value", theme).on_click(cx.listener(|this, _, _, cx| this.compare(true, cx))),
                )
                .child(
                    button("close-compare", "Close", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.compare = None;
                        cx.notify();
                    })),
                )
        });
        let header = row_div(false, false, theme)
            .h(px(HEADER_HEIGHT))
            .bg(rgb(theme.muted))
            .border_b_1()
            .border_color(rgb(theme.border))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(theme.muted_foreground))
            .child(fixed_cell(GUTTER_WIDTH, "#".into()))
            .child(cell_div(CELL_WIDTHS[0], "value".into()))
            .child(cell_div(CELL_WIDTHS[1], "label".into()))
            .child(cell_div(CELL_WIDTHS[2], "state".into()))
            .child(cell_div(CELL_WIDTHS[3], "typed".into()));
        let list = uniform_list(
            "rows",
            self.visible_count(),
            cx.processor(|this, range: Range<usize>, _window, cx| {
                range
                    .map(|index| this.row_at_position(index, cx))
                    .collect::<Vec<Stateful<Div>>>()
            }),
        )
        .track_scroll(&self.scroll)
        .flex_1();
        let palette = self.palette_open.then(|| self.palette_overlay(cx));
        let grid = div()
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .child(strip)
            .children(filter_row)
            .children(compare_bar)
            .child(header)
            .child(list);
        let content = match self.tabs.current().clone() {
            Tab::Home => self.home_page(theme, cx),
            Tab::Query => self.query_page(theme, grid, cx),
            Tab::Table(name) if name == DATA_TABLE => grid,
            Tab::Table(name) => self.placeholder_page(&name, theme),
        };
        let sidebar = self.sidebar_open.then(|| self.sidebar(theme, cx));
        let toolbar = self.toolbar(theme, cx);
        let tab_bar = self.tab_bar(theme, cx);
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .text_size(px(13.0))
            .when(!self.font.is_empty(), |root| root.font_family(self.font.clone()))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                if this.resizing_sidebar {
                    this.sidebar_width = f32::from(event.position.x).clamp(180.0, 480.0);
                    cx.notify();
                }
                if this.resizing_split {
                    let height = f32::from(window.viewport_size().height) - 72.0 - 24.0;
                    this.result_ratio = result_ratio_from_drag(f32::from(event.position.y), 72.0, height);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &gpui::MouseUpEvent, _, cx| {
                    let was_resizing =
                        std::mem::take(&mut this.resizing_sidebar) | std::mem::take(&mut this.resizing_split);
                    if was_resizing {
                        cx.notify();
                    }
                }),
            )
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .min_h_0()
                    .children(sidebar)
                    .child(div().flex_1().flex().flex_col().min_w_0().child(tab_bar).child(content)),
            )
            .child(self.status_bar(theme))
            .children(palette)
    }
}

impl GridView {
    fn palette_overlay(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        let actions = self.palette_actions();
        let shown = palette_matches(self.palette_input.read(cx).content(), &actions);
        let selected = self.palette_index.min(shown.len().saturating_sub(1));
        let list = shown.iter().enumerate().map(|(position, index)| {
            let row = div().px_2().py_1().rounded_md().child(actions[*index].label.clone());
            if position == selected {
                row.bg(rgb(theme.active_row))
            } else {
                row
            }
        });
        let empty = shown.is_empty().then(|| {
            div()
                .px_2()
                .text_color(rgb(theme.muted_foreground))
                .child("No matching command")
        });
        div().absolute().top(px(90.0)).w_full().flex().justify_center().child(
            div()
                .w(px(520.0))
                .p_2()
                .gap_1()
                .flex()
                .flex_col()
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.background))
                .shadow_lg()
                .child(self.palette_input.clone())
                .children(list)
                .children(empty),
        )
    }

    fn row_at_position(&self, position: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::current(cx);
        if let Some(diff) = &self.compare {
            let Some(row) = diff.rows.get(position) else {
                return row_div(false, false, theme).id(position);
            };
            return row_div(position % 2 == 1, false, theme)
                .id(position)
                .child(fixed_cell(70.0, row.change.to_string()))
                .child(cell_div(CELL_WIDTHS[0], cell_preview(&row.cells[0])))
                .child(cell_div(CELL_WIDTHS[1], cell_preview(&row.cells[1])))
                .child(cell_div(CELL_WIDTHS[2], cell_preview(&row.cells[2])))
                .child(cell_div(CELL_WIDTHS[3], cell_preview(&row.cells[3])));
        }
        let dataset_index = self.base() + position as u64;
        let source = self.source_row(dataset_index);
        let cells = self.source.row(source);
        let selected = self
            .selection
            .is_some_and(|selection| selection.contains(dataset_index));
        row_div(position % 2 == 1, selected, theme)
            .id(position)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.click_row(dataset_index, event.modifiers.shift);
                    cx.notify();
                }),
            )
            .child(fixed_cell(GUTTER_WIDTH, (dataset_index + 1).to_string()).text_color(rgb(theme.muted_foreground)))
            .child(self.data_cell(source, 0, position, &cells[0], cx))
            .child(self.data_cell(source, 1, position, &cells[1], cx))
            .child(self.data_cell(source, 2, position, &cells[2], cx))
            .child(self.data_cell(source, 3, position, &cells[3], cx))
    }
}

fn main() {
    let started = Instant::now();
    let total = rows_from(std::env::var("BOOKIE_PROTOTYPE_ROWS").ok().as_deref());
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
                    let view = cx.new(|cx| GridView::new(total, started, window, cx));
                    let focus = view.read(cx).focus.clone();
                    focus.focus(window, cx);
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
