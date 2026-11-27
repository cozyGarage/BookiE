use bookie_core::Environment;
use bookie_gpui::font_settings;
use bookie_gpui::keymap::Command;
use bookie_gpui::shell::{SidebarFilter, Tab, TreeRow, table_label, tree_rows_grouped, tree_rows_in};
pub(crate) const SQL: [&str; 3] = [
    "SELECT value, label, state, typed",
    "FROM synthetic_rows",
    "ORDER BY value;",
];
use gpui::{
    Context, Decorations, Div, MouseDownEvent, ScrollStrategy, Stateful, Window, WindowControls, div, prelude::*, px,
    rgb, uniform_list,
};
use gpui_kit::TestSupportExt as _;

use crate::GridView;
use crate::assets::icon;
use crate::surfaces::{Bar, ButtonKind, badge, button, compact_icon_button, icon_button, tip, toggle_icon_button};
use crate::ui_theme::ThemeExt;
use bookie_gpui::theme::{Theme, size};

pub(crate) const TOOLBAR_HEIGHT: f32 = 36.0;
pub(crate) const TAB_HEIGHT: f32 = 28.0;
pub(crate) const STATUS_HEIGHT: f32 = 24.0;

fn expand_sidebar_button(tooltip: String, theme: Theme, cx: &mut Context<GridView>) -> Stateful<Div> {
    icon_button("sidebar-open-top-left", "chevrons-right", tooltip, theme).on_click(cx.listener(|this, _, _, cx| {
        this.sidebar.open = true;
        cx.notify();
    }))
}

fn sidebar_toggle(
    id: &'static str,
    name: &'static str,
    tooltip: &'static str,
    on: bool,
    theme: Theme,
    cx: &mut Context<GridView>,
    on_click: impl Fn(&mut GridView, &mut Context<GridView>) + 'static,
) -> Stateful<Div> {
    toggle_icon_button(id, name, tooltip, on, Bar::Compact, theme).on_click(cx.listener(move |this, _, _, cx| {
        on_click(this, cx);
        cx.notify();
    }))
}

#[derive(Debug, Default, Eq, PartialEq)]
struct ToolbarWindowControls {
    minimize: bool,
    maximize: bool,
    close: bool,
}

fn toolbar_window_controls(decorations: Decorations, supported: WindowControls) -> ToolbarWindowControls {
    if decorations == Decorations::Server {
        return ToolbarWindowControls::default();
    }

    ToolbarWindowControls {
        minimize: supported.minimize,
        maximize: supported.maximize,
        close: true,
    }
}

pub(crate) use crate::surfaces::environment_color;

pub(crate) fn environment_tint(environment: Environment, theme: Theme) -> u32 {
    environment_color(environment, theme).map_or(theme.background, |color| mix(theme.background, color, 0.18))
}

fn environment_chip(environment: Environment, theme: Theme) -> Option<Div> {
    environment_color(environment, theme).map(|color| {
        badge(environment.display_name().to_uppercase(), color).debug_selector(|| "environment-chip".into())
    })
}

pub(crate) fn mix(base: u32, color: u32, amount: f32) -> u32 {
    let channel = |value: u32, shift: u32| ((value >> shift) & 0xff) as f32;
    let blend = |shift: u32| {
        let mixed = channel(base, shift) + (channel(color, shift) - channel(base, shift)) * amount.clamp(0.0, 1.0);
        (mixed.round() as u32) << shift
    };
    blend(16) | blend(8) | blend(0)
}

impl GridView {
    pub(crate) fn live_color(&self) -> Option<u32> {
        let saved = self
            .live
            .as_ref()
            .and_then(|live| live.saved.as_ref())
            .map(|(id, _)| *id);
        self.connections
            .iter()
            .find(|connection| Some(connection.id) == saved)
            .and_then(|connection| connection.color.as_deref())
            .and_then(bookie_app_services::read_session::connection_color_rgb)
    }

    fn window_tint(&self, theme: Theme) -> u32 {
        match self.live_color() {
            Some(color) => mix(theme.background, color, 0.18),
            None => environment_tint(self.live_environment(), theme),
        }
    }

    pub(crate) fn live_environment(&self) -> Environment {
        let saved = self
            .live
            .as_ref()
            .and_then(|live| live.saved.as_ref())
            .map(|(id, _)| *id);
        self.connections
            .iter()
            .find(|connection| Some(connection.id) == saved)
            .map_or(Environment::Local, |connection| connection.environment)
    }
}

#[cfg(test)]
mod environment_tests {
    use super::*;

    #[test]
    fn a_tint_moves_part_of_the_way_towards_the_environment_colour() {
        assert_eq!(mix(0xffffff, 0x000000, 0.5), 0x808080);
        assert_eq!(mix(0x102030, 0x102030, 0.7), 0x102030);
        let theme = Theme::light();
        assert_eq!(environment_tint(Environment::Local, theme), theme.background);
        assert_ne!(environment_tint(Environment::Prod, theme), theme.background);
        assert_eq!(environment_color(Environment::Prod, theme), Some(theme.destructive));
    }

    #[test]
    fn no_environment_uses_the_link_colour() {
        for theme in [Theme::light(), Theme::dark()] {
            for environment in [
                Environment::Local,
                Environment::Dev,
                Environment::Staging,
                Environment::Prod,
            ] {
                assert_ne!(
                    environment_color(environment, theme),
                    Some(theme.info),
                    "{environment:?}"
                );
            }
        }
    }
}

impl GridView {
    pub fn toolbar(&self, theme: Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        div()
            .debug_selector(|| "main-toolbar".into())
            .flex()
            .items_center()
            .justify_between()
            .h(px(TOOLBAR_HEIGHT))
            .px_2()
            .flex_none()
            .bg(rgb(self.window_tint(theme)))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.home_button(theme, cx))
                    .children((!self.sidebar.open).then(|| {
                        expand_sidebar_button(self.labelled_tip("Show sidebar", Command::ToggleSidebar), theme, cx)
                    }))
                    .children(environment_chip(self.live_environment(), theme))
                    .child(self.toolbar_command_button("new-connection", "database", theme, cx))
                    .child(self.toolbar_command_button("query", "file-plus", theme, cx))
                    .child(self.toolbar_import_export_menu(theme, cx)),
            )
            .child(self.toolbar_right(theme, window, cx))
    }

    pub(crate) fn labelled_tip(&self, label: &str, command: Command) -> String {
        bookie_gpui::keymap::with_shortcut(label, self.keymap.binding(command))
    }

    fn toolbar_command_button(
        &self,
        id: &'static str,
        name: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = bookie_gpui::palette::menu_label(id).unwrap_or(id);
        let tooltip =
            Command::for_action(id).map_or_else(|| label.to_string(), |command| self.labelled_tip(label, command));
        let control = if id == "query" { "new-query" } else { id };
        icon_button(control, name, tooltip, theme).on_click(cx.listener(move |this, _, window, cx| {
            this.run_palette_id(id, window, cx);
            cx.notify();
        }))
    }

    fn overflow_menu(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::DropdownMenu;
        let groups = crate::menus::command_groups(self, crate::menus::OVERFLOW_GROUPS);
        crate::surfaces::menu_button(
            "more",
            crate::surfaces::MenuTrigger::new("More", Bar::Small).icon("ellipsis"),
            theme,
        )
        .dropdown_menu(crate::menus::command_menu(cx.entity().downgrade(), groups))
    }

    fn toolbar_right(&self, theme: Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        let controls = if cfg!(target_os = "macos") {
            ToolbarWindowControls::default()
        } else {
            toolbar_window_controls(window.window_decorations(), window.window_controls())
        };

        div()
            .flex()
            .items_center()
            .gap_1()
            .child(self.overflow_menu(theme, cx))
            .child(
                icon_button("palette", "command", self.command_tip(Command::Palette), theme)
                    .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx))),
            )
            .child(
                icon_button(
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
            .when(controls.minimize, |this| {
                this.child(icon_button("win-min", "minus", "Minimize", theme).on_click(cx.listener(
                    |_, _, window, _| {
                        window.minimize_window();
                    },
                )))
            })
            .when(controls.maximize, |this| {
                this.child(
                    icon_button("win-max", "square", "Maximize", theme).on_click(cx.listener(|_, _, window, _| {
                        window.zoom_window();
                    })),
                )
            })
            .when(controls.close, |this| {
                this.child(
                    icon_button("win-close", "x", "Close", theme).on_click(cx.listener(|_, _, window, _| {
                        window.remove_window();
                    })),
                )
            })
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
        let fields_chevron = |table: &bookie_core::TableInfo| {
            self.live
                .is_some()
                .then(|| self.sidebar.fields.iter().any(|(open, _)| open == table))
        };
        let (label, chevron) = match &row {
            TreeRow::Connection { open } => (self.catalog.connection.clone(), Some(*open)),
            TreeRow::Database { open } => (self.catalog.database.clone(), Some(*open)),
            TreeRow::Schema { name, open, .. } => (name.clone(), Some(*open)),
            TreeRow::Table(table) if depth >= 3 => (table.name.clone(), fields_chevron(table)),
            TreeRow::Table(table) => (table_label(table), fields_chevron(table)),
        };
        let current = matches!(&row, TreeRow::Table(table) if open_table == &Tab::Table(table.clone()));
        let line = div()
            .id(("tree", index))
            .debug_selector(move || format!("sidebar-row-{index}"))
            .relative()
            .children(indent_guides(depth, theme))
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .h(px(size::TREE_ROW))
            .pl(px(tree_indent(depth as f32)))
            .gap_1()
            .rounded(px(theme.radius))
            .tooltip(tip(label.clone()))
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
            .children(
                matches!(row, TreeRow::Connection { .. })
                    .then(|| self.live_color())
                    .flatten()
                    .map(|color| connection_dot(color, "sidebar-connection-colour")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            )
            .children(match &row {
                TreeRow::Table(table) => self.table_sizes.get(table.schema.as_deref(), &table.name).map(|bytes| {
                    let name = table.name.clone();
                    div()
                        .debug_selector(move || format!("sidebar-size-{name}"))
                        .flex_none()
                        .pr_1()
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(theme.muted_foreground))
                        .child(bookie_app_services::table_sizes::size_label(bytes))
                }),
                _ => None,
            });
        let line = if current {
            line.bg(rgb(theme.accent)).font_weight(gpui::FontWeight::MEDIUM)
        } else {
            line.hover(move |style| style.bg(rgb(theme.accent)))
        };
        let menu_table = match &row {
            TreeRow::Table(table) => Some(table.clone()),
            _ => None,
        };
        line.when_some(menu_table, |line, table| {
            line.on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, _: &gpui::MouseDownEvent, window, cx| {
                    this.open_table_menu(&table, window, cx);
                }),
            )
        })
        .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
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

    fn sidebar_lines(&self, search: &str) -> Vec<SidebarLine> {
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
        let mut lines = Vec::with_capacity(rows.len());
        for (index, (depth, row)) in rows.into_iter().enumerate() {
            let table = match &row {
                TreeRow::Table(table) => Some(table.clone()),
                _ => None,
            };
            lines.push(SidebarLine::Tree { index, depth, row });
            if let Some(table) = table {
                let fields = self.field_count(&table);
                let table = std::rc::Rc::new(table);
                lines.extend((0..fields).map(|field| SidebarLine::Field {
                    table: table.clone(),
                    depth,
                    field,
                }));
            }
        }
        if search.trim().is_empty() {
            lines.extend(self.catalog_lines().into_iter().map(SidebarLine::Catalog));
        }
        lines
    }

    fn sidebar_tree_view(&self, theme: Theme, search: &str, cx: &mut Context<Self>) -> Div {
        let lines = std::rc::Rc::new(self.sidebar_lines(search));
        let pending = self.sidebar.reveal.borrow_mut().take();
        if let Some(pending) = pending {
            match lines.iter().position(
                |line| matches!(line, SidebarLine::Tree { row: TreeRow::Table(table), .. } if *table == pending),
            ) {
                Some(position) => self.sidebar.scroll.scroll_to_item(position, ScrollStrategy::Nearest),
                None => *self.sidebar.reveal.borrow_mut() = Some(pending),
            }
        }
        let empty = (!search.trim().is_empty() && self.sidebar_matches(search) == 0).then(|| {
            div()
                .p_3()
                .text_color(rgb(theme.muted_foreground))
                .child("No matching table")
        });
        let count = lines.len();
        let list = uniform_list(
            "sidebar-tree",
            count,
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                let open_table = this.tabs.current().clone();
                range
                    .filter_map(|position| match &lines[position] {
                        SidebarLine::Tree { index, depth, row } => Some(
                            this.sidebar_tree_row(*index, *depth, row.clone(), &open_table, theme, cx)
                                .into_any_element(),
                        ),
                        SidebarLine::Field { table, depth, field } => this
                            .field_row(table, *depth, *field, theme)
                            .map(IntoElement::into_any_element),
                        SidebarLine::Catalog(line) => Some(this.catalog_line(line, theme, cx)),
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.sidebar.scroll)
        .flex_1()
        .min_h_0()
        .px_2();
        div().flex().flex_col().flex_1().min_h_0().child(list).children(empty)
    }

    fn toolbar_import_export_menu(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::DropdownMenu as _;
        let owner = cx.entity().downgrade();
        let groups = crate::menus::command_groups(self, crate::menus::IMPORT_EXPORT_GROUPS);
        crate::surfaces::menu_button(
            "import-export-menu",
            crate::surfaces::MenuTrigger::new("Import or export connections", Bar::Small).icon("arrow-down-up"),
            theme,
        )
        .dropdown_menu(crate::menus::command_menu(owner, groups))
    }

    fn sidebar_header_action_icons(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .child(
                compact_icon_button("header-collapse-all", "chevrons-down-up", "Collapse all", theme).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.collapse_sidebar_sections();
                        cx.notify();
                    }),
                ),
            )
            .child(
                compact_icon_button(
                    "collapse-sidebar",
                    "chevrons-left",
                    self.labelled_tip("Hide sidebar", Command::ToggleSidebar),
                    theme,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.sidebar.open = false;
                    cx.notify();
                })),
            )
    }

    fn sidebar_connection_switcher(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};

        let active = self.live.as_deref().and_then(|live| {
            live.saved
                .as_ref()
                .map(|(id, name)| (live.driver_id == "sqlite", *id, name.as_str()))
        });
        let active_id = active.map(|(_, id, _)| id);
        let full_name = active.map_or("Connections", |(_, _, name)| name);
        let name = active.map_or("Connections", |(is_sqlite, _, name)| {
            connection_display_name(is_sqlite, name)
        });
        let connected = self.live.is_some();
        let status = connection_status(connected);
        let owner = cx.entity().downgrade();
        let entries: Vec<_> = self
            .connections
            .iter()
            .map(|saved| (saved.id, saved.driver_id == "sqlite", saved.name.clone()))
            .collect();
        let new_connection = bookie_gpui::palette::menu_label("new-connection").unwrap_or_default();

        crate::surfaces::menu_button(
            "sidebar-connection-switcher",
            crate::surfaces::MenuTrigger::new(format!("{full_name} · {status}"), Bar::Compact)
                .leading(connection_status_dot(connected, theme))
                .label(name)
                .accessible_name(format!("{full_name}, {status}. Choose connection")),
            theme,
        )
        .w_full()
        .dropdown_menu(move |mut menu, _, _| {
            for (id, is_sqlite, full_name) in &entries {
                let (id, owner) = (*id, owner.clone());
                let name = connection_display_name(*is_sqlite, full_name);
                let connected = active_id == Some(id);
                let label = format!("{name} · {}", connection_status(connected));
                let full_name = full_name.clone();
                menu = menu.item(
                    PopupMenuItem::element(move |_, _| {
                        div()
                            .id(format!("sidebar-connection-entry-{id}"))
                            .debug_selector(|| "sidebar-connection-entry".into())
                            .test_support()
                            .aria_label(label.clone())
                            .tooltip(tip(full_name.clone()))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(connection_status_dot(connected, theme))
                            .child(div().min_w_0().truncate().child(label.clone()))
                    })
                    .checked(connected)
                    .on_click(move |_, _, cx| {
                        if !connected {
                            let _ = owner.update(cx, |this, cx| {
                                this.connect(bookie_gpui::live::Origin::Saved(id.to_string()), cx);
                            });
                        }
                    }),
                );
            }
            if !entries.is_empty() {
                menu = menu.separator();
            }
            let owner = owner.clone();
            menu.item(
                crate::menus::item(new_connection, Some("database"), None, false).on_click(move |_, window, cx| {
                    let _ = owner.update(cx, |this, cx| this.run_palette_id("new-connection", window, cx));
                }),
            )
        })
    }

    fn sidebar_header(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .gap_1()
            .min_h(px(size::LIST_ROW))
            .py_1()
            .px_2()
            .border_b_1()
            .border_color(rgb(theme.border))
            .text_color(rgb(theme.muted_foreground))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.sidebar_connection_switcher(theme, cx)),
            )
            .child(self.sidebar_header_action_icons(theme, cx))
    }

    fn sidebar_search_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .p_2()
            .flex()
            .items_center()
            .gap_1()
            .child(div().flex_1().min_w_0().child(self.sidebar.search.clone()))
            .child(sidebar_toggle(
                "sidebar-locate",
                "crosshair",
                "Locate the open table",
                false,
                theme,
                cx,
                |this, cx| this.locate_active_table(cx),
            ))
            .child(sidebar_toggle(
                "sidebar-filter",
                "sliders-horizontal",
                "Regular expression",
                self.sidebar.regex,
                theme,
                cx,
                |this, cx| this.toggle_sidebar_regex(cx),
            ))
            .child(sidebar_toggle(
                "sidebar-active-only",
                "circle",
                "Show only the active table",
                self.sidebar.active_only,
                theme,
                cx,
                |this, _| this.toggle_sidebar_active_only(),
            ))
    }

    pub(crate) fn toggle_sidebar_active_only(&mut self) {
        self.sidebar.active_only = !self.sidebar.active_only;
        self.status = if self.sidebar.active_only {
            "Showing only the active table".into()
        } else {
            "Showing every table".into()
        };
    }

    pub(crate) fn collapse_sidebar_sections(&mut self) {
        let collapse = self.sidebar.connection_open || self.sidebar.database_open;
        self.sidebar.connection_open = !collapse;
        self.sidebar.database_open = !collapse;
    }

    fn sidebar_resize_handle(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("sidebar-resize")
            .debug_selector(|| "sidebar-resize".into())
            .absolute()
            .top_0()
            .right_0()
            .h_full()
            .w(px(size::SPLITTER))
            .cursor_col_resize()
            .hover(move |style| style.bg(rgb(theme.focus_ring)))
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
            self.status = "That search is not a valid regular expression".into();
            return;
        }
        self.sidebar.regex = !self.sidebar.regex;
        self.status = if self.sidebar.regex {
            "Sidebar search uses a regular expression".into()
        } else {
            "Sidebar search matches plain text".into()
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

    fn home_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let on_home = self.tabs.current() == &Tab::Home;
        toggle_icon_button("home-button", "house", "Open home", on_home, Bar::Small, theme)
            .debug_selector(|| "home-button".into())
            .on_click(cx.listener(|this, _, _, cx| {
                this.tabs.open_home();
                this.reload_history(cx);
                this.sync_grid_owner();
                cx.notify();
            }))
    }

    pub fn tab_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let active = self.tabs.active;
        let tabs = self
            .tabs
            .items
            .iter()
            .enumerate()
            .filter(|(_, tab)| **tab != Tab::Home)
            .map(|(index, tab)| self.render_tab_item(index, tab, active, theme, cx));
        div()
            .id("workspace-tabs")
            .debug_selector(|| "workspace-tabs".into())
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
        Tab::History => "clock",
        Tab::AuditLog => "shield",
        Tab::Diagram => "database",
        Tab::Query => "play",
        Tab::Table(_) => "table",
    }
}

fn tab_close_button(index: usize, theme: Theme, cx: &mut Context<GridView>) -> Stateful<Div> {
    compact_icon_button(("close", index), "x", "Close tab", theme).on_mouse_down(
        gpui::MouseButton::Left,
        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            this.close_tab(index, cx);
            this.show_current_table(window, cx);
            cx.stop_propagation();
            cx.notify();
        }),
    )
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
            .debug_selector(move || format!("tab-{index}"))
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
                if selected {
                    theme.foreground
                } else {
                    theme.muted_foreground
                },
            ))
            .children(
                matches!(tab, Tab::Table(_) | Tab::Query)
                    .then(|| self.live_color())
                    .flatten()
                    .map(|color| connection_dot(color, "tab-connection-colour")),
            )
            .child(tab.title())
            .when(dirty, |el| el.child(div().text_color(rgb(theme.warning)).child("•")));
        item = if selected {
            item.bg(rgb(theme.background))
                .text_color(rgb(theme.foreground))
                .border_b_2()
                .border_color(rgb(theme.foreground))
        } else {
            item.text_color(rgb(theme.muted_foreground))
                .hover(move |style| style.bg(rgb(theme.accent)))
        };
        if index > 0 {
            item = item.child(tab_close_button(index, theme, cx));
        }
        item.on_click(cx.listener(move |this, _, window, cx| {
            this.select_tab(index, window, cx);
            cx.notify();
        }))
    }

    pub(crate) fn select_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Tab::Table(name)) = self.tabs.items.get(index).cloned()
            && self.live.is_some()
            && name != self.catalog.grid_table
        {
            self.open_table_named(&name, window, cx);
            return;
        }
        self.tabs.active = index;
        if index == 0 {
            self.reload_history(cx);
        }
        self.sync_grid_owner();
        if self.tabs.shows_grid_for(&self.catalog.grid_table) {
            window.focus(&self.focus, cx);
        }
    }

    pub fn settings_page(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let fonts = self.font_group(view.clone(), cx);
        let mcp = self.mcp.group(view.clone());
        let dark_mode = gpui_kit::component::setting::SettingField::switch(
            |cx| Theme::current(cx).dark,
            move |on, cx| {
                if Theme::current(cx).dark != on {
                    Theme::current(cx).toggled().install(cx);
                }
                view.update(cx, |this, _| this.dark_mode = on);
            },
        );
        let chosen = if self.fonts.settings.theme.is_empty() {
            if Theme::current(cx).dark {
                crate::picker::BUILT_IN_DARK
            } else {
                crate::picker::BUILT_IN_LIGHT
            }
            .to_string()
        } else {
            self.fonts.settings.theme.clone()
        };
        let picker_view = cx.entity();
        let colour_theme = gpui_kit::component::setting::SettingField::render(move |_, _, _| {
            let view = picker_view.clone();
            button(ButtonKind::Ghost, "choose-theme", "Choose…", theme).on_click(move |_, window, cx| {
                view.update(cx, |this, cx| {
                    this.open_picker(crate::palette_state::PaletteMode::Themes, window, cx);
                    cx.notify();
                })
            })
        });
        let keyboard = self.keyboard_group(cx.entity(), theme);
        let themes = self.theme_group(cx.entity(), theme, cx);
        let pages = [
            gpui_kit::component::setting::SettingPage::new("Appearance")
                .default_open(true)
                .group(
                    gpui_kit::component::setting::SettingGroup::new()
                        .item(
                            gpui_kit::component::setting::SettingItem::new("Color theme", colour_theme)
                                .description(format!("Current: {chosen}")),
                        )
                        .item(gpui_kit::component::setting::SettingItem::new("Dark theme", dark_mode)),
                )
                .group(themes),
            gpui_kit::component::setting::SettingPage::new("Fonts").group(fonts),
            gpui_kit::component::setting::SettingPage::new("Keyboard").group(keyboard),
            gpui_kit::component::setting::SettingPage::new("MCP").group(mcp),
        ];
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .justify_center()
            .items_start()
            .p_6()
            .bg(rgb(theme.background))
            .child(
                gpui::svg()
                    .path("backdrop/contours.svg")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .text_color(rgb(theme.border)),
            )
            .child(
                crate::surfaces::card(theme)
                    .relative()
                    .debug_selector(|| "settings-card".into())
                    .w_full()
                    .max_w(px(size::HOME_WIDTH))
                    .h_full()
                    .min_h_0()
                    .bg(rgb(theme.popover))
                    .child(
                        div()
                            .px_4()
                            .pt_4()
                            .pb_2()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Settings"),
                    )
                    .child(
                        div().flex_1().min_h_0().child(
                            gpui_kit::component::setting::Settings::new((
                                "bookie-settings",
                                self.settings_page.1 as usize,
                            ))
                            .sidebar_width(px(size::SETTINGS_SIDEBAR_WIDTH))
                            .default_selected_index(gpui_kit::component::setting::SelectIndex {
                                page_ix: self.settings_page.0 as usize,
                                group_ix: None,
                            })
                            .header_style(&gpui::StyleRefinement {
                                display: Some(gpui::Display::None),
                                ..Default::default()
                            })
                            .pages(pages),
                        ),
                    ),
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
        for (input, value) in self
            .font_inputs
            .iter()
            .zip([&self.fonts.settings.ui_family, &self.fonts.settings.editor_family])
        {
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
            .relative()
            .flex_1()
            .flex()
            .justify_center()
            .p_6()
            .bg(rgb(theme.background))
            .child(
                gpui::svg()
                    .path("backdrop/contours.svg")
                    .debug_selector(|| "home-backdrop".into())
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .text_color(rgb(theme.border)),
            )
            .child(
            div()
                .relative()
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
mod window_control_tests {
    use super::*;

    #[test]
    fn server_decorations_use_native_window_controls() {
        let controls = toolbar_window_controls(Decorations::Server, WindowControls::default());

        assert_eq!(controls, ToolbarWindowControls::default());
    }

    #[test]
    fn client_decorations_only_show_controls_supported_by_the_window_manager() {
        let controls = toolbar_window_controls(
            Decorations::Client {
                tiling: gpui::Tiling::default(),
            },
            WindowControls {
                minimize: false,
                maximize: true,
                fullscreen: false,
                window_menu: false,
            },
        );

        assert_eq!(
            controls,
            ToolbarWindowControls {
                minimize: false,
                maximize: true,
                close: true,
            }
        );
    }
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

fn connection_status(connected: bool) -> &'static str {
    if connected { "Connected" } else { "Disconnected" }
}

fn connection_display_name(is_sqlite: bool, name: &str) -> &str {
    if !is_sqlite {
        return name;
    }
    name.rsplit(['/', '\\'])
        .next()
        .filter(|file_name| !file_name.is_empty())
        .unwrap_or(name)
}

fn connection_status_dot(connected: bool, theme: Theme) -> Div {
    let color = if connected {
        theme.success
    } else {
        theme.muted_foreground
    };
    connection_dot(
        color,
        if connected {
            "connection-connected"
        } else {
            "connection-disconnected"
        },
    )
}

pub(crate) fn connection_dot(color: u32, selector: &'static str) -> Div {
    div()
        .debug_selector(move || selector.into())
        .flex_none()
        .size(px(size::SM))
        .rounded_full()
        .bg(rgb(color))
}

#[cfg(test)]
mod connection_display_name_tests {
    use super::connection_display_name;

    #[test]
    fn only_sqlite_paths_are_reduced_to_a_nonempty_file_name() {
        assert_eq!(connection_display_name(true, "/tmp/orders.sqlite"), "orders.sqlite");
        assert_eq!(connection_display_name(true, r"C:\data\orders.sqlite"), "orders.sqlite");
        assert_eq!(connection_display_name(true, "orders.sqlite"), "orders.sqlite");
        assert_eq!(connection_display_name(true, "/tmp/"), "/tmp/");
        assert_eq!(connection_display_name(false, "/db/orders"), "/db/orders");
    }
}

enum SidebarLine {
    Tree {
        index: usize,
        depth: usize,
        row: TreeRow,
    },
    Field {
        table: std::rc::Rc<bookie_core::TableInfo>,
        depth: usize,
        field: usize,
    },
    Catalog(crate::sidebar_catalog::CatalogLine),
}

pub(crate) fn tree_indent(depth: f32) -> f32 {
    size::SM + size::TREE_INDENT * depth
}

pub(crate) fn guide_left(level: usize) -> f32 {
    tree_indent(level as f32) + size::ICON / 2.0
}

pub(crate) fn indent_guides(depth: usize, theme: Theme) -> impl Iterator<Item = Div> {
    (0..depth).map(move |level| {
        div()
            .debug_selector(move || format!("sidebar-guide-{level}"))
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(guide_left(level)))
            .w(px(size::HAIRLINE))
            .bg(rgb(theme.border))
    })
}

#[cfg(test)]
mod guide_tests {
    #[test]
    fn a_guide_runs_under_the_centre_of_its_parent_chevron() {
        assert_eq!(super::guide_left(0), 15.0);
        assert_eq!(super::guide_left(2), 43.0);
    }
}
