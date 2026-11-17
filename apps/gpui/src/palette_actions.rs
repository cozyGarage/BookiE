use super::*;

impl GridView {
    pub(crate) fn palette_actions(&self) -> Vec<Action> {
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
                    &format!("Open {}", table_label(&self.catalog.grid_table)),
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

    pub(crate) fn palette_shown(&self, cx: &App) -> Vec<usize> {
        palette_matches(self.palette_input.read(cx).content(), &self.palette_actions())
    }

    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_open = true;
        self.palette_fonts = false;
        self.palette_index = 0;
        self.palette_input.update(cx, |input, cx| input.set_content("", cx));
        window.focus(&self.palette_input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_open = false;
        self.palette_fonts = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn run_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
}
