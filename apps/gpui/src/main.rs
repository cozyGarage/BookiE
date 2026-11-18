use std::time::{Duration, Instant};

mod assets;
mod catalog_nav;
mod chrome;
mod copy_selection;
mod detail_panel;
mod find_filter;
mod grid_edit;
mod grid_table;
mod keys;
mod palette_actions;
mod render;
mod runs;
mod scroll_pace;
mod services;
mod text_input;
mod ui_theme;

use bookie_app_services::result_copy::{self, CopyContext, CopyFormat};
use bookie_gpui::core_bridge::column_infos_for;
use bookie_gpui::font_settings;
use bookie_gpui::keymap::Keymap;
use bookie_gpui::messages::{MessageKind, MessageLog};
use bookie_gpui::model::*;
use bookie_gpui::palette::{Action, matches as palette_matches, step as palette_step};
use bookie_gpui::selection::Selection;
use bookie_gpui::shell::{
    Catalog, DEFAULT_RESULT_RATIO, ResultView, Run, Tab, Tabs, next_run, result_ratio_from_drag, table_label,
};
use bookie_gpui::source::{Source, Synthetic};
use bookie_gpui::theme::{Style, Theme, size};
use bookie_results::grid_cursor::Cursor;
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

#[derive(Clone, Copy)]
enum Work {
    Load,
    Scan,
}

struct Ticket {
    generation: u64,
    cancel: CancellationToken,
}

impl Ticket {
    fn new() -> Self {
        Self {
            generation: 0,
            cancel: CancellationToken::new(),
        }
    }

    fn invalidate(&mut self) {
        self.generation += 1;
        self.cancel.cancel();
    }
}

struct GridView {
    table: gpui::Entity<gpui_kit::component::table::TableState<grid_table::GridDelegate>>,
    _table_events: gpui::Subscription,
    keystroke_hook: Option<gpui::Subscription>,
    focus_editor: bool,
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
    palette_delimiter: Option<bookie_gpui::csv_delimiter::Parsed>,
    csv_delimiter: Option<bookie_core::export::CustomDelimiter>,
    palette_limit: Option<bookie_gpui::export_limit::Parsed>,
    export_limit: Option<usize>,
    check_histories: Vec<(String, bookie_app_services::checks::History)>,
    sidebar_open: bool,
    import_menu_open: bool,
    sidebar_regex: bool,
    sidebar_active_only: bool,
    sidebar_width: f32,
    resizing_sidebar: bool,
    connection_open: bool,
    database_open: bool,
    sidebar_search: Entity<TextInput>,
    cursor: MatchCursor,
    find_capped: bool,
    loads: Ticket,
    scans: Ticket,
    compare: Option<std::sync::Arc<Diff>>,
    status: String,
    selection: Option<Selection>,
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

    fn init_query_inputs(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (
        Entity<TextInput>,
        Entity<TextInput>,
        Entity<TextInput>,
        Entity<EditorState>,
    ) {
        let find = cx.new(|cx| TextInput::new("Find", cx));
        let filter = cx.new(|cx| TextInput::new("label contains 7", cx));
        cx.subscribe(&find, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Changed => {
                this.invalidate_scans();
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
                this.invalidate_scans();
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
        (find, filter, editor, sql_input)
    }

    fn init_chrome_inputs(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<TextInput>, Entity<TextInput>) {
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
                    let typed = this.palette_input.read(cx).content().to_string();
                    this.palette_address = bookie_gpui::net_check::parse_address(&typed);
                    this.palette_delimiter = bookie_gpui::csv_delimiter::parse_command(&typed);
                    this.palette_limit = bookie_gpui::export_limit::parse_command(&typed);
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
        (sidebar_search, palette_input)
    }

    fn init_grid_table(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (
        Entity<gpui_kit::component::table::TableState<grid_table::GridDelegate>>,
        FocusHandle,
        gpui::Subscription,
    ) {
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
                    let cursor = bookie_results::grid_cursor::Cursor {
                        row: *row,
                        column: *column,
                    };
                    this.cell_cursor = Some(cursor);
                    let dataset = this.base() + *row as u64;
                    this.status = match this.selection {
                        Some(selection) if selection.focus == dataset => selection.label(),
                        _ => cursor.label(&this.column_names()),
                    };
                    cx.notify();
                }
                gpui_kit::component::table::TableEvent::DoubleClickedRow(row) => {
                    this.edit_clicked_row(*row, window, cx);
                }
                _ => {}
            },
        );
        (table, table_focus, table_events)
    }

    fn with_source(source: Source, started: Instant, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let total = source.total();
        let (find, filter, editor, sql_input) = Self::init_query_inputs(window, cx);
        let (sidebar_search, palette_input) = Self::init_chrome_inputs(window, cx);
        let (table, table_focus, table_events) = Self::init_grid_table(window, cx);
        let mut font_names = cx.text_system().all_font_names();
        font_names.sort();
        font_names.dedup();
        GridView {
            fonts: font_settings::load(),
            table,
            _table_events: table_events,
            focus: table_focus,
            table_source: source.clone(),
            source,
            mode: Mode::All,
            pager: Pager::new(total),
            find,
            filter,
            editor,
            sql_input,
            sidebar_search,
            palette_input,
            keystroke_hook: None, editing: None, cell_cursor: None,
            palette_address: None, palette_pattern: None, palette_delimiter: None, csv_delimiter: None, palette_limit: None, export_limit: None, compare: None,
            selection: None, pace: None, last_frame: None,
            focus_editor: false, continuous: false, filter_visible: false,
            grid_is_query: false, find_reset: false, palette_open: false,
            detail_open: false, palette_fonts: false, import_menu_open: false,
            sidebar_regex: false, sidebar_active_only: false, find_capped: false,
            measuring: false, first_frame_reported: false,
            sidebar_open: true, connection_open: true, database_open: true, result_open: true,
            resizing_split: false, resizing_sidebar: false,
            edits: Default::default(), table_edits: Default::default(),
            catalog: Catalog::synthetic(), live: None, keymap: Keymap::default(),
            tabs: Tabs::default(), tab_scroll: ScrollHandle::new(), run_scroll: ScrollHandle::new(),
            messages: MessageLog::default(), scroll: UniformListScrollHandle::new(),
            runs: vec![Run { number: 1, rows: total, elapsed_ms: None }],
            run_edits: vec![grid_edit::Edits::new()], run_sources: vec![None],
            active_run: 0, next_run_number: 2, palette_index: 0, viewport_rows: 10,
            dark_mode: Theme::current(cx).dark, result_view: ResultView::Table,
            result_ratio: DEFAULT_RESULT_RATIO, sidebar_width: 340.0,
            font: default_font(&font_names).into(), font_names,
            connections: Vec::new(), check_histories: Vec::new(), frame_intervals: Vec::new(),
            cursor: MatchCursor::default(), loads: Ticket::new(), scans: Ticket::new(),
            status: String::new(), started,
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
        work_kind: Work,
        cx: &mut Context<Self>,
        work: impl FnOnce(CancellationToken) -> T + Send + 'static,
        apply: impl FnOnce(&mut Self, T) + 'static,
    ) {
        match work_kind {
            Work::Load => self.invalidate_background(),
            Work::Scan => self.invalidate_scans(),
        }
        let ticket = match work_kind {
            Work::Load => &mut self.loads,
            Work::Scan => &mut self.scans,
        };
        let generation = ticket.generation;
        let cancel = CancellationToken::new();
        ticket.cancel = cancel.clone();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { work(cancel) }).await;
            this.update(cx, |this, cx| {
                let current = match work_kind {
                    Work::Load => this.loads.generation,
                    Work::Scan => this.scans.generation,
                };
                if current == generation {
                    apply(this, result);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn invalidate_background(&mut self) {
        self.loads.invalidate();
        self.invalidate_scans();
    }

    fn invalidate_scans(&mut self) {
        self.scans.invalidate();
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
}

fn button(id: &'static str, label: impl Into<String>, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(size::COMPACT_CONTROL))
        .px_2()
        .rounded(px(size::CONTROL_RADIUS))
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
mod typed_value_ui_tests;
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
    use bookie_gpui::live::{LiveSession, Origin};
    let table = std::env::var("BOOKIE_PROTOTYPE_TABLE").ok()?;
    let origin = match (
        std::env::var("BOOKIE_PROTOTYPE_FILE").ok(),
        std::env::var("BOOKIE_PROTOTYPE_CONNECTION").ok(),
    ) {
        (Some(path), _) => Origin::File(path.into()),
        (None, Some(connection)) => Origin::Saved(connection),
        (None, None) => return None,
    };
    let session = match LiveSession::open(&origin) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("could not open live source: {error}");
            std::process::exit(2);
        }
    };
    let Some(target) = session
        .tables
        .iter()
        .find(|candidate| candidate.name == table || table_label(candidate) == table)
    else {
        eprintln!("could not find table {table}");
        std::process::exit(2);
    };
    match session.load_table(target, bookie_gpui::live::ROW_CAP) {
        Ok(live) => Some(std::sync::Arc::new(live)),
        Err(error) => {
            eprintln!("could not read {table}: {error}");
            std::process::exit(2);
        }
    }
}
