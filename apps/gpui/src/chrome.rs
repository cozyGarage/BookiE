use bookie_gpui::font_settings;
use bookie_gpui::messages::MessageKind;
use bookie_gpui::shell::{ResultView, Run, SidebarFilter, Tab, TreeRow, table_label, tree_rows_in};
const SQL: [&str; 3] = [
    "SELECT value, label, state, typed",
    "FROM synthetic_rows",
    "ORDER BY value;",
];
use gpui::{Context, Div, MouseDownEvent, Stateful, Window, div, prelude::*, px, rgb};

use crate::assets::icon;
use crate::ui_theme::ThemeExt;
use crate::{GridView, thousands};
use bookie_gpui::theme::Theme;

pub(crate) const TOOLBAR_HEIGHT: f32 = 40.0;
pub(crate) const TAB_HEIGHT: f32 = 32.0;
pub(crate) const STATUS_HEIGHT: f32 = 24.0;

fn ghost(id: impl Into<gpui::ElementId>, label: impl Into<String>, theme: Theme) -> Stateful<Div> {
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

fn expand_sidebar_button(theme: Theme, cx: &mut Context<GridView>) -> Stateful<Div> {
    div()
        .id("sidebar-open-top-left")
        .flex()
        .items_center()
        .justify_center()
        .w(px(32.0))
        .h(px(28.0))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon("chevrons-right", theme.muted_foreground))
        .on_click(cx.listener(|this, _, _, cx| {
            this.sidebar_open = true;
            cx.notify();
        }))
}

fn import_menu(theme: Theme, cx: &mut Context<GridView>) -> Div {
    let item = |id: &'static str, label: &'static str, cx: &mut Context<GridView>| {
        div()
            .id(id)
            .px_3()
            .py_1()
            .cursor_pointer()
            .text_color(rgb(theme.foreground))
            .hover(move |style| style.bg(rgb(theme.accent)))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.import_menu_open = false;
                this.status = format!("{label} is not available in this prototype yet.");
                cx.notify();
            }))
    };
    div()
        .absolute()
        .top(px(40.0))
        .left(px(8.0))
        .w(px(220.0))
        .flex()
        .flex_col()
        .py_1()
        .rounded(px(theme.radius))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.background))
        .shadow_lg()
        .occlude()
        .child(item("import-dbx", "Import DBX Config", cx))
        .child(item("import-navicat", "Import Navicat NCX", cx))
        .child(item("import-dbeaver", "Import DBeaver", cx))
        .child(item("import-datagrip", "Import DataGrip", cx))
        .child(item("import-rdm", "Import RDM", cx))
        .child(item("import-anotherrdm", "Import AnotherRDM", cx))
        .child(item("export-connections", "Export Connections", cx))
}

fn toggle_icon(
    id: &'static str,
    name: &'static str,
    on: bool,
    theme: Theme,
    cx: &mut Context<GridView>,
    on_click: impl Fn(&mut GridView, &mut Context<GridView>) + 'static,
) -> Stateful<Div> {
    let button = header_icon(id, name, theme).on_click(cx.listener(move |this, _, _, cx| {
        on_click(this, cx);
        cx.notify();
    }));
    if on { button.bg(rgb(theme.accent)) } else { button }
}

fn header_icon(id: &'static str, name: &'static str, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(24.0))
        .h(px(24.0))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon(name, theme.muted_foreground))
}

fn window_button(id: &'static str, glyph: &'static str, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(32.0))
        .h(px(28.0))
        .cursor_pointer()
        .text_color(rgb(theme.foreground))
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(glyph)
}

fn ghost_icon(id: &'static str, name: &'static str, label: impl Into<String>, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .group(id)
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
        .child(
            div()
                .invisible()
                .group_hover(id, |style| style.visible())
                .child(label.into()),
        )
}

fn toolbar_icon(id: &'static str, name: &'static str, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(32.0))
        .h(px(28.0))
        .rounded(px(theme.radius))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon(name, theme.muted_foreground))
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

fn home_stat_card(label: &str, icon_name: &'static str, value: &str, theme: Theme) -> Div {
    card(theme)
        .flex_1()
        .p_4()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_color(rgb(theme.muted_foreground))
                .child(icon(icon_name, theme.muted_foreground))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(22.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(value.to_string()),
        )
}

fn home_shortcut(id: &'static str, icon_name: &'static str, label: &'static str, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .h(px(28.0))
        .px_2()
        .rounded(px(theme.radius))
        .cursor_pointer()
        .text_color(rgb(theme.foreground))
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon(icon_name, theme.muted_foreground))
        .child(label)
}

impl GridView {
    pub fn toolbar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
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
                    .children((!self.sidebar_open).then(|| expand_sidebar_button(theme, cx)))
                    .child(
                        toolbar_icon("new-connection", "database", theme).on_click(cx.listener(|this, _, _, cx| {
                            this.tabs.open_home();
                            this.sync_grid_owner();
                            cx.notify();
                        })),
                    )
                    .child(
                        toolbar_icon("new-query", "file-plus", theme).on_click(cx.listener(|this, _, window, cx| {
                            this.tabs.open_query();
                            this.sync_grid_owner();
                            if this.live.is_some() {
                                this.focus_editor = true;
                            } else {
                                window.focus(&this.focus, cx);
                            }
                            cx.notify();
                        })),
                    ),
            )
            .child(self.toolbar_right(theme, cx))
    }

    fn toolbar_right(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                toolbar_icon("palette", "command", theme)
                    .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx))),
            )
            .child(
                toolbar_icon("theme-toggle", if theme.dark { "sun" } else { "moon" }, theme).on_click(cx.listener(
                    |this, _, window, cx| {
                        this.dark_mode = !this.dark_mode;
                        Theme::current(cx).toggled().install(cx);
                        window.refresh();
                    },
                )),
            )
            .child(
                window_button("win-min", "\u{2013}", theme).on_click(cx.listener(|_, _, window, _| {
                    window.minimize_window();
                })),
            )
            .child(
                window_button("win-max", "\u{25a1}", theme).on_click(cx.listener(|_, _, window, _| {
                    window.zoom_window();
                })),
            )
            .child(
                window_button("win-close", "\u{00d7}", theme).on_click(cx.listener(|_, _, window, _| {
                    window.remove_window();
                })),
            )
    }

    fn sidebar_tree_row(
        &self,
        index: usize,
        depth: usize,
        row: TreeRow,
        open_table: &Tab,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (label, chevron) = match &row {
            TreeRow::Connection { open } => (self.catalog.connection.clone(), Some(*open)),
            TreeRow::Database { open } => (self.catalog.database.clone(), Some(*open)),
            TreeRow::Table(table) => (table_label(table), None),
        };
        let current = matches!(&row, TreeRow::Table(table) if open_table == &Tab::Table(table.clone()));
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
                match &row {
                    TreeRow::Connection { .. } => "database",
                    TreeRow::Database { .. } => "folder",
                    TreeRow::Table(_) => "table",
                },
                theme.muted_foreground,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            );
        let line = if current {
            line.bg(rgb(theme.accent)).font_weight(gpui::FontWeight::MEDIUM)
        } else {
            line.hover(move |style| style.bg(rgb(theme.accent)))
        };
        line.on_click(cx.listener(move |this, _, window, cx| {
            match &row {
                TreeRow::Connection { .. } => this.connection_open = !this.connection_open,
                TreeRow::Database { .. } => this.database_open = !this.database_open,
                TreeRow::Table(table) => this.open_table_named(table, window, cx),
            }
            cx.notify();
        }))
    }

    fn sidebar_tree_view(&self, theme: Theme, search: &str, cx: &mut Context<Self>) -> Div {
        let filter = SidebarFilter {
            regex: self.sidebar_regex,
            active_only: self.sidebar_active_only,
        };
        let rows = tree_rows_in(&self.catalog, search, self.connection_open, self.database_open, filter);
        let open_table = self.tabs.current().clone();
        let tree: Vec<_> = rows
            .into_iter()
            .enumerate()
            .map(|(index, (depth, row))| self.sidebar_tree_row(index, depth, row, &open_table, theme, cx))
            .collect();
        let empty = (!search.trim().is_empty() && self.sidebar_matches(search) == 0).then(|| {
            div()
                .p_3()
                .text_color(rgb(theme.muted_foreground))
                .child("No matching table")
        });
        div().flex().flex_col().px_2().children(tree).children(empty)
    }

    fn sidebar_header(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .min_h(px(36.0))
            .py_1()
            .px_2()
            .border_b_1()
            .border_color(rgb(theme.border))
            .text_color(rgb(theme.muted_foreground))
            .child(
                div()
                    .id("import-export-menu")
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .child(icon("arrow-down-up", theme.muted_foreground).flex_none())
                    .children(
                        (self.sidebar_width >= 320.0)
                            .then(|| div().flex_1().min_w_0().child("Import / Export")),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.import_menu_open = !this.import_menu_open;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(header_icon("header-collapse-all", "chevrons-down-up", theme))
                    .child(header_icon("header-new-folder", "folder-plus", theme))
                    .child(header_icon("header-refresh", "refresh-cw", theme))
                    .child(
                        header_icon("collapse-sidebar", "chevrons-left", theme).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.sidebar_open = false;
                                cx.notify();
                            },
                        )),
                    ),
            )
    }

    fn sidebar_search_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .p_2()
            .flex()
            .items_center()
            .gap_1()
            .child(div().flex_1().min_w_0().child(self.sidebar_search.clone()))
            .child(toggle_icon(
                "sidebar-locate",
                "crosshair",
                false,
                theme,
                cx,
                |this, cx| this.locate_active_table(cx),
            ))
            .child(toggle_icon(
                "sidebar-filter",
                "sliders-horizontal",
                self.sidebar_regex,
                theme,
                cx,
                |this, cx| this.toggle_sidebar_regex(cx),
            ))
            .child(toggle_icon(
                "sidebar-active-only",
                "circle",
                self.sidebar_active_only,
                theme,
                cx,
                |this, _| {
                    this.sidebar_active_only = !this.sidebar_active_only;
                    this.status = if this.sidebar_active_only {
                        "Showing only the active table.".into()
                    } else {
                        "Showing every table.".into()
                    };
                },
            ))
    }

    fn sidebar_resize_handle(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
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
            )
    }

    pub fn sidebar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let search = self.sidebar_search.read(cx).content().to_string();
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
            .child(self.sidebar_header(theme, cx))
            .children(self.import_menu_open.then(|| import_menu(theme, cx)))
            .child(self.sidebar_search_bar(theme, cx))
            .child(self.sidebar_tree_view(theme, &search, cx))
            .child(self.sidebar_resize_handle(theme, cx))
    }

    fn sidebar_matches(&self, search: &str) -> usize {
        tree_rows_in(
            &self.catalog,
            search,
            true,
            true,
            SidebarFilter {
                regex: self.sidebar_regex,
                active_only: self.sidebar_active_only,
            },
        )
        .iter()
        .filter(|(_, row)| matches!(row, TreeRow::Table(_)))
        .count()
    }

    pub(crate) fn toggle_sidebar_regex(&mut self, cx: &mut Context<Self>) {
        let search = self.sidebar_search.read(cx).content().to_string();
        if !self.sidebar_regex && !bookie_gpui::shell::sidebar_regex_ok(&search) {
            self.status = "That search is not a valid regular expression.".into();
            return;
        }
        self.sidebar_regex = !self.sidebar_regex;
        self.status = if self.sidebar_regex {
            "Sidebar search uses a regular expression.".into()
        } else {
            "Sidebar search matches plain text.".into()
        };
    }

    pub(crate) fn locate_active_table(&mut self, cx: &mut Context<Self>) {
        self.sidebar_open = true;
        self.connection_open = true;
        self.database_open = true;
        self.sidebar_active_only = false;
        self.sidebar_regex = false;
        let name = self.catalog.grid_table.name.clone();
        let label = table_label(&self.catalog.grid_table);
        self.sidebar_search.update(cx, |input, cx| input.set_content(&name, cx));
        self.status = format!("Located {label}");
    }

    pub fn tab_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let active = self.tabs.active;
        let tabs = self
            .tabs
            .items
            .iter()
            .enumerate()
            .map(|(index, tab)| self.render_tab_item(index, tab, active, theme, cx));
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
}

fn tab_icon_name(tab: &Tab) -> &'static str {
    match tab {
        Tab::Home => "house",
        Tab::Settings => "settings",
        Tab::Query => "play",
        Tab::Table(_) => "table",
    }
}

fn tab_close_button(index: usize, theme: Theme, cx: &mut Context<GridView>) -> Stateful<Div> {
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
        .child(icon("x", theme.muted_foreground))
}

impl GridView {
    fn render_tab_item(
        &self,
        index: usize,
        tab: &Tab,
        active: usize,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
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
                tab_icon_name(tab),
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
            item = item.child(tab_close_button(index, theme, cx));
        }
        item.on_click(cx.listener(move |this, _, window, cx| {
            this.tabs.active = index;
            if let Tab::Table(name) = this.tabs.current().clone()
                && this.live.is_some()
                && name != this.catalog.grid_table
            {
                this.open_table_named(&name, window, cx);
                cx.notify();
                return;
            }
            this.sync_grid_owner();
            if this.tabs.shows_grid_for(&this.catalog.grid_table) {
                window.focus(&this.focus, cx);
            }
            cx.notify();
        }))
    }

    pub fn settings_page(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let fonts = self.font_group(view.clone());
        let dark_mode = gpui_kit::component::setting::SettingField::switch(
            |cx| Theme::current(cx).dark,
            move |on, cx| {
                if Theme::current(cx).dark != on {
                    Theme::current(cx).toggled().install(cx);
                }
                view.update(cx, |this, _| this.dark_mode = on);
            },
        );
        let page = gpui_kit::component::setting::SettingPage::new("Appearance")
            .default_open(true)
            .group(
                gpui_kit::component::setting::SettingGroup::new()
                    .item(gpui_kit::component::setting::SettingItem::new("Dark theme", dark_mode)),
            )
            .group(fonts);
        div()
            .flex_1()
            .min_h_0()
            .bg(rgb(theme.background))
            .child(gpui_kit::component::setting::Settings::new("bookie-settings").page(page))
    }

    fn font_group(&self, view: gpui::Entity<Self>) -> gpui_kit::component::setting::SettingGroup {
        use gpui_kit::component::setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem};
        let size_options = || NumberFieldOptions {
            min: f64::from(font_settings::MIN_SIZE),
            max: f64::from(font_settings::MAX_SIZE),
            step: 1.0,
        };
        let family = |get: fn(&font_settings::FontSettings) -> &String,
                      set: fn(&mut font_settings::FontSettings, String)| {
            let (read, write) = (view.clone(), view.clone());
            SettingField::input(
                move |cx| get(&read.read(cx).fonts).clone().into(),
                move |value: gpui::SharedString, cx| {
                    write.update(cx, |this, cx| {
                        this.edit_fonts(|fonts| set(fonts, value.to_string()), cx)
                    });
                },
            )
        };
        let size = |get: fn(&font_settings::FontSettings) -> f32, set: fn(&mut font_settings::FontSettings, f32)| {
            let (read, write) = (view.clone(), view.clone());
            SettingField::number_input(
                size_options(),
                move |cx| f64::from(get(&read.read(cx).fonts)),
                move |value: f64, cx| {
                    write.update(cx, |this, cx| {
                        this.edit_fonts(|fonts| set(fonts, font_settings::clamp_size(value)), cx)
                    });
                },
            )
        };
        SettingGroup::new()
            .item(SettingItem::new(
                "UI font",
                family(|f| &f.ui_family, |f, v| f.ui_family = v),
            ))
            .item(SettingItem::new(
                "UI font size",
                size(|f| f.ui_size, |f, v| f.ui_size = v),
            ))
            .item(SettingItem::new(
                "Editor font",
                family(|f| &f.editor_family, |f, v| f.editor_family = v),
            ))
            .item(SettingItem::new(
                "Editor font size",
                size(|f| f.editor_size, |f, v| f.editor_size = v),
            ))
    }

    pub fn edit_fonts(&mut self, edit: impl FnOnce(&mut font_settings::FontSettings), cx: &mut Context<Self>) {
        edit(&mut self.fonts);
        if let Err(error) = font_settings::save(&self.fonts) {
            self.status = format!("Could not save font settings: {error}");
        }
        cx.notify();
    }

    pub fn home_page(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex_1()
            .flex()
            .justify_center()
            .p_6()
            .bg(rgb(theme.background))
            .child(
                div()
                    .pt(px(96.0))
                    .w(px(980.0))
                    .max_w_full()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(self.home_stats_row(theme))
                    .child(
                        div()
                            .flex()
                            .gap_4()
                            .child(self.home_quick_start_card(theme, cx))
                            .child(self.home_shortcuts_card(theme, cx)),
                    )
                    .child(self.home_query_history_card(theme))
                    .child(
                        div()
                            .w_full()
                            .text_color(rgb(theme.muted_foreground))
                            .child("BookiE desktop frontend. Connect to a database or select a table from the sidebar to browse data."),
                    ),
            )
    }

    fn home_stats_row(&self, theme: Theme) -> Div {
        let unique_drivers = self
            .connections
            .iter()
            .map(|c| c.driver_id.as_str())
            .chain(self.live.as_ref().map(|l| l.driver_id.as_str()))
            .collect::<std::collections::HashSet<_>>()
            .len()
            .max(1);
        div()
            .flex()
            .gap_4()
            .child(home_stat_card(
                "Connections",
                "database",
                &self.connections.len().max(1).to_string(),
                theme,
            ))
            .child(home_stat_card(
                "Connected",
                "play",
                &usize::from(self.live.is_some() || self.connections.is_empty()).to_string(),
                theme,
            ))
            .child(home_stat_card(
                "Database Types",
                "package",
                &unique_drivers.to_string(),
                theme,
            ))
    }

    fn home_quick_start_card(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        card(theme)
            .flex_1()
            .child(card_title("Quick Start", theme))
            .child(
                div()
                    .p_2()
                    .flex()
                    .flex_col()
                    .children(self.connections.iter().enumerate().map(|(index, saved)| {
                        let name = saved.name.clone();
                        ghost(
                            ("saved-connection", index),
                            format!("{} · {}", saved.name, saved.driver_id),
                            theme,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.connect(bookie_gpui::live::Origin::Saved(name.clone()), cx);
                            cx.notify();
                        }))
                    }))
                    .when(!self.catalog.tables.is_empty(), |section| {
                        section.child(
                            ghost(
                                "quick-open",
                                format!(
                                    "{} → {}",
                                    self.catalog.connection,
                                    table_label(&self.catalog.grid_table)
                                ),
                                theme,
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                let name = this.catalog.grid_table.clone();
                                this.open_table_named(&name, window, cx);
                                cx.notify();
                            })),
                        )
                    }),
            )
    }

    fn home_shortcuts_card(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        card(theme)
            .w(px(300.0))
            .child(card_title("Shortcuts", theme))
            .child(
                div()
                    .p_2()
                    .flex()
                    .flex_col()
                    .child(
                        home_shortcut("s-new", "plus", "New Query", theme)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.tabs.open_query();
                                this.sync_grid_owner();
                                window.focus(&this.focus, cx);
                            })),
                    )
                    .child(
                        home_shortcut("s-status", "play", "Connection Status", theme)
                            .on_click(cx.listener(|this, _, _, _| {
                                this.show_status();
                            })),
                    )
                    .child(
                        home_shortcut("s-palette", "command", "Command Palette", theme)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_palette(window, cx);
                            })),
                    ),
            )
    }

    fn home_query_history_card(&self, theme: Theme) -> Div {
        let entries = self.messages.entries();
        let content = if entries.is_empty() {
            div()
                .p_4()
                .text_color(rgb(theme.muted_foreground))
                .child("No recent statements executed yet")
        } else {
            div()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .children(entries.iter().rev().take(5).map(|entry| {
                    div()
                        .px_2()
                        .py_1()
                        .rounded(px(theme.radius))
                        .text_size(px(12.0))
                        .font_family("monospace")
                        .text_color(rgb(match entry.kind {
                            MessageKind::Error => theme.warning,
                            _ => theme.foreground,
                        }))
                        .child(entry.text.clone())
                }))
        };
        card(theme)
            .child(card_title("Query History", theme))
            .child(content)
    }

    pub fn placeholder_page(&self, name: &str, theme: Theme) -> Div {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(theme.muted_foreground))
            .child(format!(
                "No data loaded for {name}. Select an active table or run a query."
            ))
    }

    pub fn status_bar(&self, theme: Theme) -> Div {
        let right = if self.tabs.shows_grid_for(&self.catalog.grid_table) {
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
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.source.origin())
                    .when(!self.status.is_empty(), |row| {
                        row.child(div().child("·"))
                            .child(div().text_color(rgb(theme.foreground)).child(self.status.clone()))
                    }),
            )
            .child(right)
    }

    fn query_editor_toolbar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
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
                        this.run_query(cx);
                        cx.notify();
                    },
                )),
            )
            .child(
                div()
                    .text_color(rgb(theme.muted_foreground))
                    .child(self.catalog.connection.clone()),
            )
    }

    fn query_editor_pane(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .when(self.result_open, |pane| pane.h(gpui::relative(1.0 - self.result_ratio)))
            .when(!self.result_open, |pane| pane.flex_1())
            .child(self.query_editor_toolbar(theme, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .p_2()
                    .gap_3()
                    .overflow_hidden()
                    .children(self.live.is_none().then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .text_color(rgb(theme.muted_foreground))
                            .children((1..=SQL.len()).map(|number| div().h(px(20.0)).child(number.to_string())))
                    }))
                    .children(self.live.is_none().then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .children(SQL.iter().map(|line| div().h(px(20.0)).child((*line).to_string())))
                    }))
                    .children(self.live.is_some().then(|| {
                        div()
                            .flex_1()
                            .when(!self.fonts.editor_family.is_empty(), |pane| {
                                pane.font_family(self.fonts.editor_family.clone())
                            })
                            .text_size(px(self.fonts.editor_size))
                            .child(gpui_kit::component::input::Editor::new(&self.sql_input).h(gpui::relative(1.0)))
                    })),
            )
    }

    fn query_split_divider(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
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
            )
    }

    pub fn query_page(&mut self, theme: Theme, grid: Div, cx: &mut Context<Self>) -> Div {
        let editor = self.query_editor_pane(theme, cx);
        let divider = self.query_split_divider(theme, cx);
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

    fn result_tab_item(
        &self,
        index: usize,
        run: &Run,
        active: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
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
            .child(match run.measured() {
                Some(took) => format!("{} · {} rows · {took}", run.label(), thousands(run.rows)),
                None => format!("{} · {} rows", run.label(), thousands(run.rows)),
            })
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
    }

    fn result_view_switch(
        &self,
        id: &'static str,
        label: &'static str,
        view: ResultView,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.result_view == view;
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
            this.result_view = view;
            cx.notify();
        }))
    }

    fn result_header(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let run_tabs = self
            .runs
            .iter()
            .enumerate()
            .map(|(index, run)| self.result_tab_item(index, run, index == self.active_run, theme, cx));
        div()
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
            .child(self.result_view_switch("view-table", "Table Data", ResultView::Table, theme, cx))
            .child(self.result_view_switch("view-summary", "Summary", ResultView::Summary, theme, cx))
            .child(self.result_view_switch("view-messages", "Messages", ResultView::Messages, theme, cx))
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
            )
    }

    fn result_messages_body(&self, theme: Theme) -> Div {
        div().flex_1().min_h_0().flex().flex_col().child(
            div()
                .id("messages")
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .p_4()
                .overflow_y_scroll()
                .children(if self.messages.entries().is_empty() {
                    vec![div().text_color(rgb(theme.muted_foreground)).child("No messages yet")]
                } else {
                    self.messages
                        .entries()
                        .iter()
                        .rev()
                        .map(|message| {
                            let line = div().child(message.text.clone());
                            match message.kind {
                                MessageKind::Error => line.text_color(rgb(theme.warning)),
                                MessageKind::Info => line.text_color(rgb(theme.muted_foreground)),
                                MessageKind::Success => line,
                            }
                        })
                        .collect()
                }),
        )
    }

    fn result_summary_body(&self, run: &Run) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .p_4()
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Execution summary"))
            .children(self.live.is_none().then(|| format!("Statement: {}", SQL.join(" "))))
            .child(format!("Rows returned: {}", thousands(run.rows)))
            .child(match run.measured() {
                Some(took) => format!("Elapsed: {took}"),
                None => format!("Elapsed: {} ms (synthetic)", run.millis()),
            })
            .children(
                self.live
                    .is_none()
                    .then_some("Source: synthetic generator, no database"),
            )
    }

    fn result_body(&self, theme: Theme, grid: Div) -> Div {
        if self.result_view == ResultView::Messages {
            self.result_messages_body(theme)
        } else if let Some(run) = self
            .runs
            .get(self.active_run)
            .filter(|_| self.result_view == ResultView::Summary)
        {
            self.result_summary_body(run)
        } else if self.runs.is_empty() {
            div().flex_1().p_4().child("No results")
        } else {
            grid
        }
    }

    fn result_pane(&mut self, theme: Theme, grid: Div, cx: &mut Context<Self>) -> Div {
        let header = self.result_header(theme, cx);
        let body = self.result_body(theme, grid);
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
