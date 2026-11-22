use bookie_gpui::font_settings;
use bookie_gpui::shell::{SidebarFilter, Tab, TreeRow, table_label, tree_rows_grouped, tree_rows_in};
pub(crate) const SQL: [&str; 3] = [
    "SELECT value, label, state, typed",
    "FROM synthetic_rows",
    "ORDER BY value;",
];
use gpui::{Context, Div, MouseDownEvent, Stateful, Window, div, prelude::*, px, rgb};

use crate::assets::icon;
use crate::ui_theme::ThemeExt;
use crate::GridView;
use bookie_gpui::theme::{Theme, size};

pub(crate) const TOOLBAR_HEIGHT: f32 = 40.0;
pub(crate) const TAB_HEIGHT: f32 = 32.0;
pub(crate) const STATUS_HEIGHT: f32 = 24.0;

pub(crate) fn ghost(id: impl Into<gpui::ElementId>, label: impl Into<String>, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(size::SMALL_CONTROL))
        .px_2()
        .rounded(px(theme.radius))
        .cursor_pointer()
        .text_color(rgb(theme.foreground))
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(label.into())
}

pub(crate) fn primary_button(id: &'static str, label: &'static str, tip_text: &'static str, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .tooltip(tip(tip_text))
        .flex()
        .items_center()
        .h(px(size::SMALL_CONTROL))
        .px_3()
        .rounded(px(theme.radius))
        .bg(rgb(theme.foreground))
        .text_color(rgb(theme.background))
        .cursor_pointer()
        .child(label)
}

pub(crate) fn tip(label: &'static str) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    move |window, cx| gpui_component::tooltip::Tooltip::new(label).build(window, cx)
}

fn expand_sidebar_button(theme: Theme, cx: &mut Context<GridView>) -> Stateful<Div> {
    div()
        .id("sidebar-open-top-left")
        .tooltip(tip("Show sidebar"))
        .flex()
        .items_center()
        .justify_center()
        .w(px(size::FORM_CONTROL))
        .h(px(size::SMALL_CONTROL))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon("chevrons-right", theme.muted_foreground))
        .on_click(cx.listener(|this, _, _, cx| {
            this.sidebar.open = true;
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
                this.sidebar.import_menu_open = false;
                match id {
                    "import-connections" => this.pick_connection_import(cx),
                    "export-connections" => this.pick_connection_export(cx),
                    _ => {}
                }
                cx.notify();
            }))
    };
    div()
        .absolute()
        .top(px(TOOLBAR_HEIGHT))
        .left(px(size::SM))
        .w(px(size::MENU_WIDTH))
        .flex()
        .flex_col()
        .py_1()
        .rounded(px(theme.radius))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.background))
        .shadow_lg()
        .occlude()
        .child(item("import-connections", "Import Connections…", cx))
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
        .w(px(size::COMPACT_CONTROL))
        .h(px(size::COMPACT_CONTROL))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon(name, theme.muted_foreground))
}

fn window_button(id: &'static str, name: &'static str, label: &'static str, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .tooltip(tip(label))
        .flex()
        .items_center()
        .justify_center()
        .w(px(size::FORM_CONTROL))
        .h(px(size::SMALL_CONTROL))
        .cursor_pointer()
        .text_color(rgb(theme.foreground))
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon(name, theme.foreground))
}

pub(crate) fn ghost_icon(id: impl Into<gpui::ElementId>, name: &'static str, label: &'static str, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .tooltip(tip(label))
        .flex()
        .items_center()
        .justify_center()
        .w(px(size::FORM_CONTROL))
        .h(px(size::SMALL_CONTROL))
        .rounded(px(theme.radius))
        .cursor_pointer()
        .text_color(rgb(theme.foreground))
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon(name, theme.muted_foreground))
}

fn toolbar_icon(id: &'static str, name: &'static str, label: &'static str, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .tooltip(tip(label))
        .flex()
        .items_center()
        .justify_center()
        .w(px(size::FORM_CONTROL))
        .h(px(size::SMALL_CONTROL))
        .rounded(px(theme.radius))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon(name, theme.muted_foreground))
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
                    .children((!self.sidebar.open).then(|| expand_sidebar_button(theme, cx)))
                    .child(
                        toolbar_icon("new-connection", "database", "Connections", theme).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.tabs.open_home();
                                this.sync_grid_owner();
                                this.reload_history(cx);
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        toolbar_icon("new-query", "file-plus", "New query", theme).on_click(cx.listener(
                            |this, _, window, cx| {
                                this.tabs.open_query();
                                this.sync_grid_owner();
                                if this.live.is_some() {
                                    this.focus_editor = true;
                                } else {
                                    window.focus(&this.focus, cx);
                                }
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(self.toolbar_right(theme, cx))
    }

    fn overflow_menu(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::button::{Button, ButtonVariants};
        use gpui_component::menu::DropdownMenu;
        let groups = crate::menus::command_groups(self, crate::menus::OVERFLOW_GROUPS);
        Button::new("more")
            .ghost()
            .compact()
            .tooltip("More")
            .child(icon("ellipsis", theme.muted_foreground))
            .dropdown_menu(crate::menus::command_menu(cx.entity().downgrade(), groups))
    }

    fn toolbar_right(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(self.overflow_menu(theme, cx))
            .child(
                toolbar_icon("palette", "command", "Command palette (Ctrl+Shift+P)", theme)
                    .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx))),
            )
            .child(
                toolbar_icon(
                    "theme-toggle",
                    if theme.dark { "sun" } else { "moon" },
                    if theme.dark {
                        "Switch to light theme"
                    } else {
                        "Switch to dark theme"
                    },
                    theme,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.dark_mode = !this.dark_mode;
                    Theme::current(cx).toggled().install(cx);
                    window.refresh();
                })),
            )
            .child(
                window_button("win-min", "minus", "Minimize", theme).on_click(cx.listener(|_, _, window, _| {
                    window.minimize_window();
                })),
            )
            .child(
                window_button("win-max", "square", "Maximize", theme).on_click(cx.listener(|_, _, window, _| {
                    window.zoom_window();
                })),
            )
            .child(
                window_button("win-close", "x", "Close", theme).on_click(cx.listener(|_, _, window, _| {
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
            TreeRow::Schema { name, open, .. } => (name.clone(), Some(*open)),
            TreeRow::Table(table) if depth >= 3 => (table.name.clone(), None),
            TreeRow::Table(table) => (table_label(table), None),
        };
        let current = matches!(&row, TreeRow::Table(table) if open_table == &Tab::Table(table.clone()));
        let line = div()
            .id(("tree", index))
            .flex()
            .items_center()
            .h(px(size::TREE_ROW))
            .pl(px(8.0 + 14.0 * depth as f32))
            .gap_1()
            .rounded(px(theme.radius))
            .cursor_pointer()
            .child(div().w(px(size::CHEVRON)).flex_none().children(chevron.map(|open| {
                icon(
                    if open { "chevron-down" } else { "chevron-right" },
                    theme.muted_foreground,
                )
            })))
            .child(icon(
                match &row {
                    TreeRow::Connection { .. } => "database",
                    TreeRow::Database { .. } | TreeRow::Schema { .. } => "folder",
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
        line.on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
            this.click_tree_row(&row, event.click_count(), window, cx);
            cx.notify();
        }))
    }

    fn click_tree_row(&mut self, row: &TreeRow, click_count: usize, window: &mut Window, cx: &mut Context<Self>) {
        let this = self;
        match row {
            TreeRow::Connection { .. } => this.sidebar.connection_open = !this.sidebar.connection_open,
            TreeRow::Database { .. } => this.sidebar.database_open = !this.sidebar.database_open,
            TreeRow::Schema { key, .. } => this.sidebar.collapsed_schemas.toggle(key),
            TreeRow::Table(table) if click_count >= 2 || this.live.is_none() => {
                this.open_table_named(table, window, cx)
            }
            TreeRow::Table(table) => this.toggle_fields(table, cx),
        }
    }

    fn sidebar_tree_view(&self, theme: Theme, search: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let filter = SidebarFilter {
            regex: self.sidebar.regex,
            active_only: self.sidebar.active_only,
        };
        let rows = tree_rows_grouped(
            &self.catalog,
            search,
            self.sidebar.connection_open,
            self.sidebar.database_open,
            filter,
            &self.sidebar.collapsed_schemas,
        );
        let open_table = self.tabs.current().clone();
        let pending = self.sidebar.reveal.borrow().clone();
        let mut tree: Vec<gpui::AnyElement> = Vec::new();
        for (index, (depth, row)) in rows.into_iter().enumerate() {
            let table = match &row {
                TreeRow::Table(table) => Some(table.clone()),
                _ => None,
            };
            if table.is_some() && table == pending {
                self.sidebar.scroll.scroll_to_item(tree.len());
                self.sidebar.reveal.borrow_mut().take();
            }
            tree.push(
                self.sidebar_tree_row(index, depth, row, &open_table, theme, cx)
                    .into_any_element(),
            );
            if let Some(table) = table {
                tree.extend(
                    self.field_rows(&table, depth, theme)
                        .into_iter()
                        .map(IntoElement::into_any_element),
                );
            }
        }
        if search.trim().is_empty() {
            tree.extend(self.catalog_rows(theme, cx));
        }
        let empty = (!search.trim().is_empty() && self.sidebar_matches(search) == 0).then(|| {
            div()
                .p_3()
                .text_color(rgb(theme.muted_foreground))
                .child("No matching table")
        });
        div()
            .id("sidebar-tree")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.sidebar.scroll)
            .flex()
            .flex_col()
            .px_2()
            .children(tree)
            .children(empty)
    }

    fn sidebar_import_export_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("import-export-menu")
            .tooltip(tip("Import / Export connections"))
            .flex_1()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .cursor_pointer()
            .child(icon("arrow-down-up", theme.muted_foreground).flex_none())
            .children((self.sidebar.width >= 320.0).then(|| div().flex_1().min_w_0().child("Import / Export")))
            .on_click(cx.listener(|this, _, _, cx| {
                this.sidebar.import_menu_open = !this.sidebar.import_menu_open;
                cx.notify();
            }))
    }

    fn sidebar_header_action_icons(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .child(
                header_icon("header-collapse-all", "chevrons-down-up", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        let collapse = this.sidebar.connection_open || this.sidebar.database_open;
                        this.sidebar.connection_open = !collapse;
                        this.sidebar.database_open = !collapse;
                        cx.notify();
                    },
                )),
            )
            .child(
                header_icon("header-new-folder", "folder-plus", theme).on_click(cx.listener(|this, _, _, cx| {
                    this.status = "Folder organization is coming soon".into();
                    cx.notify();
                })),
            )
            .child(
                header_icon("header-refresh", "refresh-cw", theme).on_click(cx.listener(|this, _, _, cx| {
                    this.status = "Refreshed catalog schema".into();
                    cx.notify();
                })),
            )
            .child(
                header_icon("collapse-sidebar", "chevrons-left", theme).on_click(cx.listener(|this, _, _, cx| {
                    this.sidebar.open = false;
                    cx.notify();
                })),
            )
    }

    fn sidebar_header(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .min_h(px(size::LIST_ROW))
            .py_1()
            .px_2()
            .border_b_1()
            .border_color(rgb(theme.border))
            .text_color(rgb(theme.muted_foreground))
            .child(self.sidebar_import_export_button(theme, cx))
            .child(self.sidebar_header_action_icons(theme, cx))
    }

    fn sidebar_search_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .p_2()
            .flex()
            .items_center()
            .gap_1()
            .child(div().flex_1().min_w_0().child(self.sidebar.search.clone()))
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
                self.sidebar.regex,
                theme,
                cx,
                |this, cx| this.toggle_sidebar_regex(cx),
            ))
            .child(toggle_icon(
                "sidebar-active-only",
                "circle",
                self.sidebar.active_only,
                theme,
                cx,
                |this, _| {
                    this.sidebar.active_only = !this.sidebar.active_only;
                    this.status = if this.sidebar.active_only {
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
            .w(px(size::SPLITTER))
            .cursor_col_resize()
            .hover(move |style| style.bg(rgb(theme.info)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.sidebar.resizing = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }

    pub fn sidebar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let search = self.sidebar.search.read(cx).content().to_string();
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(self.sidebar.width))
            .h_full()
            .bg(rgb(theme.sidebar))
            .border_r_1()
            .border_color(rgb(theme.border))
            .child(self.sidebar_header(theme, cx))
            .children(self.sidebar.import_menu_open.then(|| import_menu(theme, cx)))
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
                regex: self.sidebar.regex,
                active_only: self.sidebar.active_only,
            },
        )
        .iter()
        .filter(|(_, row)| matches!(row, TreeRow::Table(_)))
        .count()
    }

    pub(crate) fn toggle_sidebar_regex(&mut self, cx: &mut Context<Self>) {
        let search = self.sidebar.search.read(cx).content().to_string();
        if !self.sidebar.regex && !bookie_gpui::shell::sidebar_regex_ok(&search) {
            self.status = "That search is not a valid regular expression.".into();
            return;
        }
        self.sidebar.regex = !self.sidebar.regex;
        self.status = if self.sidebar.regex {
            "Sidebar search uses a regular expression.".into()
        } else {
            "Sidebar search matches plain text.".into()
        };
    }

    pub(crate) fn locate_active_table(&mut self, cx: &mut Context<Self>) {
        self.sidebar.open = true;
        self.sidebar.connection_open = true;
        self.sidebar.database_open = true;
        self.sidebar.active_only = false;
        self.sidebar.regex = false;
        let name = self.catalog.grid_table.name.clone();
        let label = table_label(&self.catalog.grid_table);
        self.sidebar.search.update(cx, |input, cx| input.set_content(&name, cx));
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

pub(crate) fn one_line(sql: &str) -> String {
    let flat: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(120) {
        Some((end, _)) => format!("{}…", &flat[..end]),
        None => flat,
    }
}

fn tab_icon_name(tab: &Tab) -> &'static str {
    match tab {
        Tab::Home => "house",
        Tab::Settings => "settings",
        Tab::NewConnection => "database",
        Tab::History => "clock",
        Tab::AuditLog => "shield",
        Tab::Diagram => "database",
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
            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                this.close_tab(index);
                this.show_current_table(window, cx);
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
        let dirty = tab_is_dirty(tab, &self.catalog.grid_table, self.cell_edit.edits.len());
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
            .child(tab.title())
            .when(dirty, |el| el.child(div().text_color(rgb(theme.warning)).child("•")));
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
            if index == 0 {
                this.reload_history(cx);
            }
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
        let fonts = self.font_group(view.clone(), cx);
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
            .child(
                gpui_kit::component::setting::Settings::new("bookie-settings")
                    .header_style(&gpui::StyleRefinement {
                        display: Some(gpui::Display::None),
                        ..Default::default()
                    })
                    .page(page),
            )
    }

    fn font_group(&self, view: gpui::Entity<Self>, cx: &gpui::App) -> gpui_kit::component::setting::SettingGroup {
        use gpui_kit::component::setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem};
        let size_options = || NumberFieldOptions {
            min: f64::from(font_settings::MIN_SIZE),
            max: f64::from(font_settings::MAX_SIZE),
            step: 1.0,
        };
        let ui_default = if self.fonts.current.is_empty() {
            gpui_kit::component::theme::Theme::global(cx).font_family.clone()
        } else {
            self.fonts.current.clone()
        };
        let editor_default = gpui_kit::component::theme::Theme::global(cx).mono_font_family.clone();
        let family = |index: usize, placeholder: gpui::SharedString| {
            let input = self.font_inputs[index].clone();
            SettingField::render(move |_, _, cx| {
                input.update(cx, |input, cx| input.set_placeholder(&placeholder, cx));
                div().w_64().child(input.clone())
            })
        };
        let size = |get: fn(&font_settings::FontSettings) -> f32, set: fn(&mut font_settings::FontSettings, f32)| {
            let (read, write) = (view.clone(), view.clone());
            SettingField::number_input(
                size_options(),
                move |cx| f64::from(get(&read.read(cx).fonts.settings)),
                move |value: f64, cx| {
                    write.update(cx, |this, cx| {
                        this.edit_fonts(|fonts| set(fonts, font_settings::clamp_size(value)), cx)
                    });
                },
            )
        };
        SettingGroup::new()
            .item(
                SettingItem::new("UI font", family(0, ui_default.clone()))
                    .description(format!("Default: {ui_default}")),
            )
            .item(SettingItem::new(
                "UI font size",
                size(|f| f.ui_size, |f, v| f.ui_size = v),
            ))
            .item(
                SettingItem::new("Editor font", family(1, editor_default.clone()))
                    .description(format!("Default: {editor_default}")),
            )
            .item(SettingItem::new(
                "Editor font size",
                size(|f| f.editor_size, |f, v| f.editor_size = v),
            ))
    }

    pub fn edit_fonts(&mut self, edit: impl FnOnce(&mut font_settings::FontSettings), cx: &mut Context<Self>) {
        edit(&mut self.fonts.settings);
        for (input, value) in self.font_inputs.iter().zip([
            &self.fonts.settings.ui_family,
            &self.fonts.settings.editor_family,
        ]) {
            if input.read(cx).content() != value {
                input.update(cx, |input, cx| input.set_content_quiet(value, cx));
            }
        }
        if let Err(error) = font_settings::save(&self.fonts.settings) {
            self.status = format!("Could not save font settings: {error}");
        }
        crate::ui_theme::apply_editor_size(self.fonts.settings.editor_size, cx);
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
                .pt(px(size::HOME_TOP))
                .w(px(size::HOME_WIDTH))
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
                .child(div().w_full().text_color(rgb(theme.muted_foreground)).child(
                    "BookiE desktop frontend. Connect to a database or select a table from the sidebar to browse data.",
                )),
        )
    }

}

fn tab_is_dirty(tab: &Tab, grid_table: &bookie_core::TableInfo, pending_edits: usize) -> bool {
    pending_edits > 0 && matches!(tab, Tab::Table(table) if table == grid_table)
}

#[cfg(test)]
mod tab_dirty_tests {
    use super::*;

    fn table(name: &str) -> bookie_core::TableInfo {
        bookie_core::TableInfo {
            schema: None,
            name: name.into(),
        }
    }

    #[test]
    fn only_the_table_tab_with_pending_edits_is_marked() {
        let open = table("customers");
        assert!(tab_is_dirty(&Tab::Table(open.clone()), &open, 1));
        assert!(!tab_is_dirty(&Tab::Table(open.clone()), &open, 0));
        assert!(!tab_is_dirty(&Tab::Table(table("orders")), &open, 3));
        assert!(!tab_is_dirty(&Tab::Query, &open, 3));
    }
}
