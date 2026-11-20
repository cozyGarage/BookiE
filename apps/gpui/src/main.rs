use std::io::Write;
use std::time::{Duration, Instant};

mod assets;
mod catalog_nav;
mod chrome;
mod home_page;
mod query_pane;
mod connection_form;
mod copy_selection;
mod detail_panel;
mod find_filter;
mod grid_edit;
mod grid_table;
mod audit_page;
mod history_view;
mod keys;
mod measure;
mod menus;
mod palette_actions;
mod palette_state;
mod edit_history;
mod favorites_ui;
mod approval_ui;
mod import_ui;
mod export_file_ui;
mod selection_stats_ui;
mod backup_ui;
mod diagram_scene;
mod diagram_ui;
mod filter_rule_ui;
mod health_ui;
mod paste_ui;
mod refresh_ui;
mod run_cursor_ui;
mod schema_diagram_ui;
mod set_cells_ui;
mod update_ui;
mod value_search_ui;
mod resolve_ui;
mod structure_ui;
mod insert_ui;
mod params_ui;
mod picker;
mod save_ui;
mod plan_view;
mod reveal;
mod panel_state;
mod render;
mod runs;
mod scroll_pace;
mod services;
mod sidebar_catalog;
mod ssh_form;
mod sidebar_fields;
mod sidebar_state;
mod text_input;
mod ui_theme;

use bookie_app_services::result_copy::{self, CopyContext, CopyFormat};
use bookie_gpui::core_bridge::column_infos_for;
use bookie_gpui::font_settings;
use bookie_gpui::keymap::Keymap;
use bookie_gpui::messages::MessageKind;
use bookie_gpui::model::*;
use bookie_gpui::palette::{Action, matches as palette_matches, step as palette_step};
use bookie_gpui::selection::Selection;
use bookie_gpui::shell::{
    Catalog, DEFAULT_RESULT_RATIO, ResultView, Run, Tab, Tabs, next_run, result_ratio_from_drag, table_label,
};
use bookie_gpui::source::{Source, Synthetic};
use bookie_gpui::theme::{Theme, size};
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
    grid_is_query: bool,
    search: panel_state::SearchState,
    cell_edit: panel_state::CellEditState,
    cell_cursor: Option<Cursor>,
    viewport_rows: usize,
    detail_open: bool,
    catalog: Catalog,
    live: Option<std::sync::Arc<bookie_gpui::live::LiveSession>>,
    connections: Vec<bookie_app_services::read_session::ConnectionSummary>,
    keymap: Keymap,
    dark_mode: bool,
    tabs: Tabs,
    tab_scroll: ScrollHandle,
    sql_input: Entity<EditorState>,
    run_tabs: panel_state::RunTabs,
    check_histories: Vec<(String, bookie_app_services::checks::History)>,
    palette: palette_state::PaletteState,
    favorites: favorites_ui::FavoritesState,
    params: params_ui::ParamsState,
    save: save_ui::SaveState,
    approval: approval_ui::ApprovalUi,
    insert: insert_ui::InsertState,
    import: import_ui::ImportState,
    structure: structure_ui::StructureState,
    resolve: resolve_ui::ResolveState,
    sidebar: sidebar_state::SidebarState,
    row_menu: Option<menus::RowMenu>,
    connection_form: connection_form::ConnectionForm,
    history: Vec<bookie_app_services::statement_history::RecentStatement>,
    history_page: history_view::HistoryPage,
    audit_page: audit_page::AuditPage,
    panel: panel_state::PanelState,
    fonts: panel_state::FontState,
    export: panel_state::ExportState,
    measure: measure::Measure,
    loads: Ticket,
    scans: Ticket,
    compare: Option<std::sync::Arc<Diff>>,
    status: String,
    selection: Option<Selection>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
}

pub(crate) struct Pace {
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

const UI_FONTS: [&str; 7] = [
    "Inter",
    "SF Pro Text",
    "Segoe UI",
    "Adwaita Sans",
    "Roboto",
    "Noto Sans",
    "Fira Sans",
];

fn installed_font_names(cx: &App) -> Vec<String> {
    let mut names = cx.text_system().all_font_names();
    names.sort();
    names.dedup();
    names
}

fn first_run(total: u64) -> Run {
    Run {
        number: 1,
        rows: total,
        elapsed_ms: None,
    }
}

fn default_font(installed: &[String]) -> String {
    let wanted = std::env::var("BOOKIE_PROTOTYPE_FONT").ok();
    wanted
        .iter()
        .map(String::as_str)
        .chain(UI_FONTS)
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
                this.search.cursor = MatchCursor::default();
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

    fn init_chrome_inputs(window: &mut Window, cx: &mut Context<Self>) -> (Entity<TextInput>, Entity<TextInput>) {
        let sidebar_search = cx.new(|cx| TextInput::new("Search tables", cx));
        cx.subscribe(&sidebar_search, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        let palette_input = cx.new(|cx| TextInput::new("Type a command", cx));
        cx.subscribe_in(
            &palette_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Changed => {
                    this.palette.index = 0;
                    let typed = this.palette.input.read(cx).content().to_string();
                    this.palette.address = bookie_gpui::net_check::parse_address(&typed);
                    this.palette.delimiter = bookie_gpui::csv_delimiter::parse_command(&typed);
                    this.palette.limit = bookie_gpui::export_limit::parse_command(&typed);
                    this.palette.pattern = this.live.as_ref().and_then(|live| match live.driver_id.as_str() {
                        "redis" => bookie_gpui::pubsub_view::parse_pattern(&typed),
                        "postgres" => bookie_gpui::pubsub_view::parse_channel(&typed),
                        _ => None,
                    });
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
                gpui_kit::component::table::TableEvent::RightClickedCell(row, column) => {
                    this.cell_cursor = Some(bookie_results::grid_cursor::Cursor {
                        row: *row,
                        column: *column,
                    });
                    let dataset = this.base() + *row as u64;
                    if !this.selection.is_some_and(|selection| selection.contains(dataset)) {
                        this.click_row(dataset, false);
                    }
                    this.open_cell_menu(window, cx);
                }
                gpui_kit::component::table::TableEvent::DoubleClickedRow(row) => {
                    this.edit_clicked_row(*row, None, window, cx);
                }
                gpui_kit::component::table::TableEvent::DoubleClickedCell(row, column) => {
                    this.edit_clicked_row(*row, Some(*column), window, cx);
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
        let font_names = installed_font_names(cx);
        let (approval, approval_inbox) = approval_ui::ApprovalUi::new();
        Self::watch_approvals(approval_inbox, cx);
        GridView {
            table,
            _table_events: table_events,
            focus: table_focus,
            table_source: source.clone(),
            source,
            mode: Mode::All,
            pager: Pager::new(total),
            sql_input,
            palette: palette_state::PaletteState::new(palette_input),
            favorites: favorites_ui::FavoritesState::default(),
            params: params_ui::ParamsState::default(),
            save: save_ui::SaveState::default(),
            approval,
            insert: insert_ui::InsertState::default(),
            import: import_ui::ImportState::default(),
            structure: structure_ui::StructureState::default(),
            resolve: resolve_ui::ResolveState::default(),
            sidebar: sidebar_state::SidebarState::new(sidebar_search),
            row_menu: None,
            connection_form: connection_form::ConnectionForm::new(cx),
            history: Vec::new(),
            history_page: history_view::HistoryPage::new(cx),
            audit_page: audit_page::AuditPage::new(cx),
            panel: panel_state::PanelState::new(DEFAULT_RESULT_RATIO),
            fonts: panel_state::FontState::new(font_settings::load(), default_font(&font_names).into(), font_names),
            export: panel_state::ExportState::default(),
            measure: measure::Measure::new(started),
            keystroke_hook: None,
            cell_cursor: None,
            compare: None,
            selection: None,
            focus_editor: false,
            continuous: false,
            grid_is_query: false,
            search: panel_state::SearchState::new(find, filter),
            cell_edit: panel_state::CellEditState::new(editor),
            detail_open: false,
            catalog: Catalog::synthetic(),
            live: None,
            keymap: Keymap::default(),
            tabs: Tabs::default(),
            tab_scroll: ScrollHandle::new(),
            scroll: UniformListScrollHandle::new(),
            run_tabs: panel_state::RunTabs::new(runs::RunSlot::new(first_run(total), None)),
            viewport_rows: 10,
            dark_mode: Theme::current(cx).dark,
            connections: Vec::new(),
            check_histories: Vec::new(),
            loads: Ticket::new(),
            scans: Ticket::new(),
            status: String::new(),
        }
    }

    fn dataset_count(&self) -> u64 {
        if matches!(self.tabs.current(), Tab::Query) && self.run_tabs.slots.is_empty() {
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
        if !self.measure.first_frame_reported {
            self.measure.first_frame_reported = true;
            let _ = writeln!(
                std::io::stdout(),
                "PROTOTYPE_FIRST_FRAME ms={}",
                self.measure.started.elapsed().as_millis()
            );
        }
        if let (true, Some(last)) = (self.measure.measuring, self.measure.last_frame) {
            self.measure.frame_intervals.push(now - last);
        }
        self.measure.last_frame = Some(now);
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

fn icon_button(id: &'static str, name: &'static str, tip: &'static str, active: bool, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .tooltip(chrome::tip(tip))
        .flex()
        .items_center()
        .justify_center()
        .w(px(size::COMPACT_CONTROL))
        .h(px(size::COMPACT_CONTROL))
        .rounded(px(size::CONTROL_RADIUS))
        .cursor_pointer()
        .when(active, |style| style.bg(rgb(theme.accent)))
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(assets::icon(
            name,
            if active {
                theme.foreground
            } else {
                theme.muted_foreground
            },
        ))
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

const STATEMENT_LOG_TARGET: &str = "sqlx::query";

fn log_filter(requested: Option<&str>) -> tracing_subscriber::EnvFilter {
    use tracing_subscriber::EnvFilter;
    let filter = requested
        .and_then(|requested| EnvFilter::try_new(requested).ok())
        .unwrap_or_else(|| EnvFilter::new("warn"));
    if requested.is_some_and(|requested| requested.contains(STATEMENT_LOG_TARGET)) {
        return filter;
    }
    match format!("{STATEMENT_LOG_TARGET}=off").parse() {
        Ok(directive) => filter.add_directive(directive),
        Err(_) => filter,
    }
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(log_filter(std::env::var("RUST_LOG").ok().as_deref()))
        .init();
}

#[cfg(test)]
mod logging_tests {
    use std::sync::{Arc, Mutex};

    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if let Ok(mut buffer) = self.0.lock() {
                buffer.extend_from_slice(bytes);
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn emitted(requested: Option<&str>) -> String {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || Capture(writer.clone()))
            .with_env_filter(super::log_filter(requested))
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "sqlx::query", "SELECT secret FROM accounts");
            tracing::warn!(target: "bookie_gpui", "ordinary warning");
        });
        let bytes = buffer.lock().map(|buffer| buffer.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn statement_text_is_not_logged_by_default_but_other_warnings_are() {
        let output = emitted(None);
        assert!(!output.contains("SELECT secret"), "{output}");
        assert!(output.contains("ordinary warning"), "{output}");
    }

    #[test]
    fn an_explicit_statement_log_request_is_respected() {
        assert!(emitted(Some("sqlx::query=warn")).contains("SELECT secret"));
    }
}

fn main() {
    let started = Instant::now();
    init_logging();
    let total = rows_from(std::env::var("BOOKIE_PROTOTYPE_ROWS").ok().as_deref());
    let live = live_source();
    let keymap = load_keymap();
    let connections = if live.is_some() {
        Vec::new()
    } else {
        bookie_gpui::live::saved_connections().unwrap_or_else(|error| {
            tracing::warn!(%error, "saved connections unavailable");
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
            ui_theme::apply_editor_size(font_settings::load().editor_size, cx);
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
                    view.update(cx, |view, cx| {
                        view.reload_history(cx);
                        view.refresh_favorites(cx);
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
                tracing::error!(%error, "could not open a window");
                std::process::exit(2);
            }
        });
}

#[cfg(test)]
mod live_ui_tests;
#[cfg(test)]
mod save_ui_tests;
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
        tracing::warn!(%error, path = %path.display(), "ignoring file");
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
            tracing::error!(%error, "could not open live source");
            std::process::exit(2);
        }
    };
    let Some(target) = session
        .tables
        .iter()
        .find(|candidate| candidate.name == table || table_label(candidate) == table)
    else {
        tracing::error!(%table, "could not find table");
        std::process::exit(2);
    };
    match session.load_table(target, bookie_gpui::live::ROW_CAP) {
        Ok(live) => Some(std::sync::Arc::new(live)),
        Err(error) => {
            tracing::error!(%table, %error, "could not read table");
            std::process::exit(2);
        }
    }
}
