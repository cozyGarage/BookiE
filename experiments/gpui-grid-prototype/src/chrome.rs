use bookie_gpui_grid_prototype::shell::{CONNECTION, DATA_TABLE, DATABASE, Tab, TreeRow, tree_rows};
const SQL: [&str; 3] = [
    "SELECT value, label, state, typed",
    "FROM synthetic_rows",
    "ORDER BY value;",
];
use gpui::{Context, Div, MouseDownEvent, Stateful, Window, div, prelude::*, px, rgb};

use crate::assets::icon;
use crate::ui_theme::ThemeExt;
use crate::{GridView, thousands};
use bookie_gpui_grid_prototype::theme::Theme;

const TOOLBAR_HEIGHT: f32 = 40.0;
const TAB_HEIGHT: f32 = 32.0;
const STATUS_HEIGHT: f32 = 24.0;

fn ghost(id: &'static str, label: impl Into<String>, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(28.0))
        .px_2()
        .rounded(px(theme.radius))
        .cursor_pointer()
        .text_color(rgb(theme.foreground))
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(label.into())
}

fn ghost_icon(id: &'static str, name: &'static str, label: impl Into<String>, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1()
        .h(px(28.0))
        .px_2()
        .rounded(px(theme.radius))
        .cursor_pointer()
        .text_color(rgb(theme.foreground))
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon(name, theme.muted_foreground))
        .child(label.into())
}

fn card(theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.background))
}

fn card_title(text: &str, theme: Theme) -> Div {
    div()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(rgb(theme.border))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(text.to_string())
}

impl GridView {
    pub fn toolbar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let inert = |id: &'static str, name: &'static str, label: &'static str, cx: &mut Context<Self>| {
            ghost_icon(id, name, label, theme).on_click(cx.listener(|this, _, _, cx| {
                this.status = "Synthetic prototype: nothing to open here.".into();
                cx.notify();
            }))
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(TOOLBAR_HEIGHT))
            .px_2()
            .flex_none()
            .bg(rgb(theme.background))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(inert("new-connection", "plus", "New Connection", cx))
                    .child(inert("data-transfer", "arrow-left-right", "Data Transfer", cx))
                    .child(inert("driver-manager", "package", "Driver Manager", cx))
                    .child(inert("plugin-center", "puzzle", "Plugin Center", cx))
                    .child(inert("more", "chevron-down", "More", cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        ghost_icon("palette", "command", "Commands", theme)
                            .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx))),
                    )
                    .child(
                        ghost_icon("sidebar-toggle", "panel-left", "Sidebar", theme).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.sidebar_open = !this.sidebar_open;
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        ghost_icon(
                            "theme-toggle",
                            if theme.dark { "sun" } else { "moon" },
                            if theme.dark { "Light" } else { "Dark" },
                            theme,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dark_mode = !this.dark_mode;
                            Theme::current(cx).toggled().install(cx);
                            window.refresh();
                        })),
                    ),
            )
    }

    pub fn sidebar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let search = self.sidebar_search.read(cx).content().to_string();
        let rows = tree_rows(&search, self.connection_open, self.database_open);
        let open_table = self.tabs.current().clone();
        let tree = rows.into_iter().enumerate().map(|(index, (depth, row))| {
            let (label, chevron) = match row {
                TreeRow::Connection { open } => (CONNECTION.to_string(), Some(open)),
                TreeRow::Database { open } => (DATABASE.to_string(), Some(open)),
                TreeRow::Table(name) => (name.to_string(), None),
            };
            let current = matches!(row, TreeRow::Table(name) if open_table == Tab::Table(name.to_string()));
            let line = div()
                .id(("tree", index))
                .flex()
                .items_center()
                .h(px(26.0))
                .pl(px(8.0 + 14.0 * depth as f32))
                .gap_1()
                .rounded(px(theme.radius))
                .cursor_pointer()
                .child(div().w(px(15.0)).flex_none().children(chevron.map(|open| {
                    icon(
                        if open { "chevron-down" } else { "chevron-right" },
                        theme.muted_foreground,
                    )
                })))
                .child(icon(
                    match row {
                        TreeRow::Connection { .. } => "database",
                        TreeRow::Database { .. } => "folder",
                        TreeRow::Table(_) => "table",
                    },
                    theme.muted_foreground,
                ))
                .child(label);
            let line = if current {
                line.bg(rgb(theme.accent)).font_weight(gpui::FontWeight::MEDIUM)
            } else {
                line.hover(move |style| style.bg(rgb(theme.accent)))
            };
            line.on_click(cx.listener(move |this, _, window, cx| {
                match row {
                    TreeRow::Connection { .. } => this.connection_open = !this.connection_open,
                    TreeRow::Database { .. } => this.database_open = !this.database_open,
                    TreeRow::Table(name) => {
                        this.tabs.open_table(name);
                        this.sync_grid_owner();
                        if this.tabs.shows_grid() {
                            window.focus(&this.focus, cx);
                        }
                    }
                }
                cx.notify();
            }))
        });
        let empty = (!search.trim().is_empty() && self.sidebar_matches(&search) == 0).then(|| {
            div()
                .p_3()
                .text_color(rgb(theme.muted_foreground))
                .child("No matching table")
        });
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .bg(rgb(theme.chrome))
            .border_r_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(36.0))
                    .px_2()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .text_color(rgb(theme.muted_foreground))
                    .child("Import / Export ⌄")
                    .child(
                        ghost("collapse-sidebar", "«", theme).on_click(cx.listener(|this, _, _, cx| {
                            this.sidebar_open = false;
                            cx.notify();
                        })),
                    ),
            )
            .child(div().p_2().child(self.sidebar_search.clone()))
            .child(div().flex().flex_col().px_2().children(tree).children(empty))
            .child(
                div()
                    .id("sidebar-resize")
                    .absolute()
                    .top_0()
                    .right_0()
                    .h_full()
                    .w(px(5.0))
                    .cursor_col_resize()
                    .hover(move |style| style.bg(rgb(theme.info)))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.resizing_sidebar = true;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
    }

    fn sidebar_matches(&self, search: &str) -> usize {
        tree_rows(search, true, true)
            .iter()
            .filter(|(_, row)| matches!(row, TreeRow::Table(_)))
            .count()
    }

    pub fn tab_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let active = self.tabs.active;
        let tabs = self.tabs.items.iter().enumerate().map(|(index, tab)| {
            let selected = index == active;
            let mut item = div()
                .id(("tab", index))
                .flex()
                .flex_none()
                .items_center()
                .gap_2()
                .h_full()
                .px_3()
                .border_r_1()
                .border_color(rgb(theme.border))
                .cursor_pointer()
                .child(icon(
                    match tab {
                        Tab::Home => "house",
                        Tab::Query => "play",
                        Tab::Table(_) => "table",
                    },
                    if selected { theme.info } else { theme.muted_foreground },
                ))
                .child(tab.title());
            item = if selected {
                item.bg(rgb(theme.background))
                    .text_color(rgb(theme.foreground))
                    .border_b_2()
                    .border_color(rgb(theme.info))
            } else {
                item.text_color(rgb(theme.muted_foreground))
                    .hover(move |style| style.bg(rgb(theme.accent)))
            };
            if index > 0 {
                item = item.child(
                    div()
                        .id(("close", index))
                        .px_1()
                        .rounded(px(theme.radius))
                        .text_color(rgb(theme.muted_foreground))
                        .hover(move |style| style.bg(rgb(theme.muted)))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.close_tab(index);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .child(icon("x", theme.muted_foreground)),
                );
            }
            item.on_click(cx.listener(move |this, _, window, cx| {
                this.tabs.active = index;
                this.sync_grid_owner();
                if this.tabs.shows_grid() {
                    window.focus(&this.focus, cx);
                }
                cx.notify();
            }))
        });
        div()
            .id("workspace-tabs")
            .flex()
            .items_center()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(TAB_HEIGHT))
            .overflow_x_scroll()
            .track_scroll(&self.tab_scroll)
            .bg(rgb(theme.chrome))
            .border_b_1()
            .border_color(rgb(theme.border))
            .children(tabs)
    }

    pub fn home_page(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let stat = |label: &str, value: &str| {
            card(theme)
                .flex_1()
                .p_4()
                .gap_1()
                .child(div().text_color(rgb(theme.muted_foreground)).child(label.to_string()))
                .child(
                    div()
                        .text_size(px(22.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(value.to_string()),
                )
        };
        let shortcut = |id: &'static str, label: &'static str, cx: &mut Context<Self>| {
            ghost(id, label, theme).on_click(cx.listener(|this, _, _, cx| {
                this.status = "Synthetic prototype: nothing to open here.".into();
                cx.notify();
            }))
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_4()
            .p_6()
            .bg(rgb(theme.background))
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(stat("Connections", "1"))
                    .child(stat("Connected", "1"))
                    .child(stat("Database Types", "1")),
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(
                        card(theme)
                            .flex_1()
                            .child(card_title("Quick Start", theme))
                            .child(div().p_2().child(
                                ghost("quick-open", format!("{CONNECTION} → {DATA_TABLE}"), theme).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.tabs.open_table(DATA_TABLE);
                                        this.sync_grid_owner();
                                        window.focus(&this.focus, cx);
                                        cx.notify();
                                    }),
                                ),
                            )),
                    )
                    .child(
                        card(theme).w(px(300.0)).child(card_title("Shortcuts", theme)).child(
                            div()
                                .p_2()
                                .flex()
                                .flex_col()
                                .child(shortcut("s-new", "+  New Connection", cx))
                                .child(shortcut("s-history", "History", cx))
                                .child(shortcut("s-import", "Import Connections", cx)),
                        ),
                    ),
            )
            .child(
                card(theme).child(card_title("Query History", theme)).child(
                    div()
                        .p_4()
                        .text_color(rgb(theme.muted_foreground))
                        .child("No SQL Library records yet"),
                ),
            )
            .child(
                div()
                    .text_color(rgb(theme.muted_foreground))
                    .child("Layout prototype with synthetic data only. Open the synthetic_rows table in the sidebar."),
            )
    }

    pub fn placeholder_page(&self, name: &str, theme: Theme) -> Div {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(theme.muted_foreground))
            .child(format!(
                "{name} has no data in this prototype. Only synthetic_rows is populated."
            ))
    }

    pub fn status_bar(&self, theme: Theme) -> Div {
        let right = if self.tabs.shows_grid() {
            format!(
                "{} rows · {}",
                thousands(self.dataset_count()),
                if theme.dark { "dark" } else { "light" }
            )
        } else {
            if theme.dark { "dark".into() } else { "light".into() }
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .flex_none()
            .h(px(STATUS_HEIGHT))
            .px_3()
            .bg(rgb(theme.chrome))
            .border_t_1()
            .border_color(rgb(theme.border))
            .text_size(px(12.0))
            .text_color(rgb(theme.muted_foreground))
            .child("Synthetic data · no database connection")
            .child(right)
    }

    pub fn query_page(&mut self, theme: Theme, grid: Div, cx: &mut Context<Self>) -> Div {
        let editor = div()
            .flex()
            .flex_col()
            .min_h_0()
            .when(self.result_open, |pane| pane.h(gpui::relative(1.0 - self.result_ratio)))
            .when(!self.result_open, |pane| pane.flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(32.0))
                    .px_2()
                    .flex_none()
                    .bg(rgb(theme.chrome))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        ghost_icon("run-query", "play", "Run  Ctrl+Enter", theme).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.run_query();
                                cx.notify();
                            },
                        )),
                    )
                    .child(div().text_color(rgb(theme.muted_foreground)).child(CONNECTION)),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .p_2()
                    .gap_3()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .text_color(rgb(theme.muted_foreground))
                            .children((1..=SQL.len()).map(|number| div().h(px(20.0)).child(number.to_string()))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .children(SQL.iter().map(|line| div().h(px(20.0)).child((*line).to_string()))),
                    ),
            );
        let divider = div()
            .id("split-divider")
            .h(px(6.0))
            .w_full()
            .flex_none()
            .cursor_row_resize()
            .bg(rgb(theme.border))
            .hover(move |style| style.bg(rgb(theme.info)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.resizing_split = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        let result = self.result_pane(theme, grid, cx);
        div()
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .child(editor)
            .children(self.result_open.then_some(divider))
            .child(result)
    }

    fn result_pane(&mut self, theme: Theme, grid: Div, cx: &mut Context<Self>) -> Div {
        let run_tabs = self.runs.iter().enumerate().map(|(index, run)| {
            let active = index == self.active_run;
            let tab = div()
                .id(("run", index))
                .flex()
                .items_center()
                .flex_none()
                .h(px(28.0))
                .px_2()
                .rounded(px(theme.radius))
                .border_1()
                .cursor_pointer()
                .child(format!("{} · {} rows", run.label(), thousands(run.rows)))
                .child(
                    div()
                        .id(("close-run", index))
                        .px_1()
                        .rounded(px(theme.radius))
                        .hover(move |style| style.bg(rgb(theme.muted)))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.close_run(index);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .child(icon("x", theme.muted_foreground)),
                );
            let tab = if active {
                tab.border_color(rgb(theme.border))
                    .bg(rgb(theme.background))
                    .shadow_sm()
            } else {
                tab.border_color(rgb(theme.muted))
                    .text_color(rgb(theme.muted_foreground))
                    .hover(move |style| style.bg(rgb(theme.accent)))
            };
            tab.on_click(cx.listener(move |this, _, _, cx| {
                this.select_run(index);
                cx.notify();
            }))
        });
        let switch = |id: &'static str, label: &'static str, selected: bool, summary: bool, cx: &mut Context<Self>| {
            let button = div()
                .id(id)
                .flex()
                .items_center()
                .h(px(24.0))
                .px_2()
                .rounded(px(theme.radius))
                .cursor_pointer()
                .child(label);
            let button = if selected {
                button.bg(rgb(theme.accent)).text_color(rgb(theme.foreground))
            } else {
                button.text_color(rgb(theme.muted_foreground))
            };
            button.on_click(cx.listener(move |this, _, _, cx| {
                this.summary_view = summary;
                cx.notify();
            }))
        };
        let header = div()
            .flex()
            .items_center()
            .gap_1()
            .h(px(40.0))
            .px_2()
            .flex_none()
            .bg(rgb(theme.chrome))
            .border_y_1()
            .border_color(rgb(theme.border))
            .child(icon("pin", theme.muted_foreground))
            .child(div().w(px(1.0)).h(px(16.0)).bg(rgb(theme.border)))
            .child(
                div()
                    .id("result-runs")
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .overflow_x_scroll()
                    .track_scroll(&self.run_scroll)
                    .children(run_tabs),
            )
            .child(switch("view-table", "Table Data", !self.summary_view, false, cx))
            .child(switch("view-summary", "Summary", self.summary_view, true, cx))
            .child(
                ghost("toggle-results", "", theme)
                    .child(icon(
                        if self.result_open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        },
                        theme.muted_foreground,
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.result_open = !this.result_open;
                        cx.notify();
                    })),
            );
        let body = if let Some(run) = self.runs.get(self.active_run).filter(|_| self.summary_view) {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .p_4()
                .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Execution summary"))
                .child(format!("Statement: {}", SQL.join(" ")))
                .child(format!("Rows returned: {}", thousands(run.rows)))
                .child(format!("Elapsed: {} ms (synthetic)", run.millis()))
                .child("Source: synthetic generator, no database")
        } else if self.runs.is_empty() {
            div().flex_1().p_4().child("No results")
        } else {
            grid
        };
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .when(self.result_open, |pane| pane.flex_1())
            .child(header)
            .children(self.result_open.then_some(body))
    }

    pub fn viewport_rows_for(&self, window: &Window) -> usize {
        ((f32::from(window.viewport_size().height) - 250.0) / crate::ROW_HEIGHT)
            .floor()
            .max(1.0) as usize
    }
}
